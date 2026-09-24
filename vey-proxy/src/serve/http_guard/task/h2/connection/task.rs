/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use h2::Reason;
use jiff::Timestamp;
use log::debug;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_io_ext::LimitedStream;

use super::{
    H2ConcurrencyStats, H2ConnectionCltWrapperStats, H2ConnectionTaskStats, H2StreamTask,
    H2TaskContext,
};
use crate::config::server::ServerConfig;
use crate::log::task::h2_connection::TaskLogForH2Connection;
use crate::serve::{ServerStats, ServerTaskNotes};

pub(crate) struct HttpGuardH2ConnectionTask<S> {
    ctx: Arc<H2TaskContext>,
    stream: Option<S>,
    task_notes: ServerTaskNotes,
    task_stats: Arc<H2ConnectionTaskStats>,
    concurrency: Arc<H2ConcurrencyStats>,
    first_stream_at: Option<Timestamp>,
}

impl<S> HttpGuardH2ConnectionTask<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub(crate) fn new(ctx: &Arc<H2TaskContext>, stream: S) -> Self {
        let mut task_notes = ServerTaskNotes::new(ctx.cc_info.clone(), None, Default::default())
            .with_site_ctx(ctx.site_ctx.clone());
        task_notes.id = ctx.connection_id;
        HttpGuardH2ConnectionTask {
            ctx: Arc::clone(ctx),
            stream: Some(stream),
            task_notes,
            task_stats: Arc::new(H2ConnectionTaskStats::default()),
            concurrency: Arc::new(H2ConcurrencyStats::default()),
            first_stream_at: None,
        }
    }

    fn log_ctx(&self) -> Option<TaskLogForH2Connection<'_>> {
        self.ctx
            .task_logger
            .as_ref()
            .map(|logger| TaskLogForH2Connection {
                logger,
                task_notes: &self.task_notes,
                connection_id: &self.ctx.connection_id,
                stream_total: self.concurrency.get_total_task(),
                stream_alive: self.concurrency.get_alive_task(),
                first_stream_at: self.first_stream_at.as_ref(),
                client_rd_bytes: self.task_stats.clt.read.get_bytes(),
                client_wr_bytes: self.task_stats.clt.write.get_bytes(),
            })
    }

    pub(crate) async fn into_running(mut self) {
        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log) = self.log_ctx()
        {
            log.log_created();
        }
        match self.run().await {
            Ok(()) => {
                if let Some(log) = self.log_ctx() {
                    log.log("finished");
                }
            }
            Err(e) => {
                debug!(
                    "{} - {} h2 connection error: {e}",
                    self.ctx.client_addr(),
                    self.ctx.server_addr()
                );
                if let Some(log) = self.log_ctx() {
                    log.log(&e.to_string());
                }
            }
        }
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let site = Arc::clone(self.ctx.site_ctx.site());
        let tenant_user = self.ctx.site_ctx.tenant_user().cloned();
        let _site_conn = site.hold_http_conn(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );

        let stream = self.stream.take().unwrap();
        let limit = self
            .ctx
            .site_ctx
            .tcp_sock_speed_limit()
            .shrink_as_smaller(&self.ctx.server_config.tcp_sock_speed_limit);
        let site_io_stats = site.stats().fetch_traffic_stats(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );
        let mut stream = LimitedStream::local_limited(
            stream,
            limit.shift_millis,
            limit.max_north,
            limit.max_south,
            H2ConnectionCltWrapperStats::new(
                &self.ctx.server_stats,
                site_io_stats,
                &self.task_stats,
            ),
        );
        if let Some(user) = &tenant_user {
            if let Some(limiter) = user.tcp_all_upload_speed_limit() {
                stream.add_global_read_limiter(limiter.clone());
            }
            if let Some(limiter) = user.tcp_all_download_speed_limit() {
                stream.add_global_write_limiter(limiter.clone());
            }
        }

        let server_builder = self.ctx.server_config.h2.build_server();
        let mut h2c = tokio::time::timeout(
            self.ctx.server_config.h2.client_handshake_timeout,
            server_builder.handshake(stream),
        )
        .await
        .map_err(|_| anyhow::anyhow!("client h2 handshake timeout"))?
        .map_err(|e| anyhow::anyhow!("client h2 handshake: {e}"))?;

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut log_interval = self.ctx.get_log_interval();
        let mut idle_count = 0;
        let idle_max = self
            .task_notes
            .task_max_idle_count(self.ctx.server_config.task_idle_max_count);

        loop {
            tokio::select! {
                biased;

                clt_r = h2c.accept() => {
                    match clt_r {
                        Some(Ok((clt_req, clt_send_rsp))) => {
                            idle_count = 0;
                            if self.first_stream_at.is_none() {
                                self.first_stream_at = Some(Timestamp::now());
                            }
                            let ctx = Arc::clone(&self.ctx);
                            let task_guard = self.concurrency.add_task();
                            let clt_stream_id = clt_send_rsp.stream_id();
                            tokio::spawn(async move {
                                H2StreamTask::new(ctx, clt_stream_id)
                                    .run(clt_req, clt_send_rsp)
                                    .await;
                                drop(task_guard);
                            });
                        }
                        Some(Err(e)) => {
                            if let Some(io) = e.get_io()
                                && io.kind() == std::io::ErrorKind::NotConnected
                            {
                                return Ok(());
                            }
                            return Err(anyhow::anyhow!("client h2 closed: {e}"));
                        }
                        None => {
                            let _ = std::future::poll_fn(|cx| h2c.poll_closed(cx)).await;
                            return Ok(());
                        }
                    }
                }
                n = idle_interval.tick() => {
                    if self.concurrency.get_alive_task() <= 0 {
                        idle_count += n;
                        if idle_count > idle_max {
                            h2c.abrupt_shutdown(Reason::NO_ERROR);
                            let _ = std::future::poll_fn(|cx| h2c.poll_closed(cx)).await;
                            return Ok(());
                        }
                    } else {
                        idle_count = 0;
                    }
                    if self.ctx.server_quit_policy.force_quit() {
                        h2c.graceful_shutdown();
                        let _ = std::future::poll_fn(|cx| h2c.poll_closed(cx)).await;
                        return Ok(());
                    }
                }
                _ = log_interval.tick() => {
                    if let Some(log) = self.log_ctx() {
                        log.log_periodic();
                    }
                }
            }
        }
    }
}
