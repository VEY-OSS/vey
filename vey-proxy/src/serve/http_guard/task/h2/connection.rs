/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use h2::Reason;
use log::debug;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_io_ext::LimitedStream;
use vey_types::route::HostMatch;

use super::CommonTaskContext;
use super::stats::{H2ConcurrencyStats, H2ConnectionCltWrapperStats};
use super::stream;
use crate::config::server::ServerConfig;
use crate::serve::ServerStats;
use crate::serve::http_guard::HttpHost;

pub(crate) struct HttpGuardH2ConnectionTask<S> {
    ctx: Arc<CommonTaskContext>,
    stream: Option<S>,
    hosts: Arc<HostMatch<Arc<HttpHost>>>,
}

impl<S> HttpGuardH2ConnectionTask<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub(crate) fn new(
        ctx: &Arc<CommonTaskContext>,
        stream: S,
        hosts: Arc<HostMatch<Arc<HttpHost>>>,
    ) -> Self {
        HttpGuardH2ConnectionTask {
            ctx: Arc::clone(ctx),
            stream: Some(stream),
            hosts,
        }
    }

    pub(crate) async fn into_running(mut self) {
        if let Err(e) = self.run().await {
            debug!(
                "{} - {} h2 connection error: {e}",
                self.ctx.client_addr(),
                self.ctx.server_addr()
            );
        }
    }

    async fn run(&mut self) -> anyhow::Result<()> {
        let site = self
            .ctx
            .pinned_site
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("h2 requires a matching tls sni site"))?;
        let _site_conn = site.hold_http_conn(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );

        let stream = self.stream.take().unwrap();
        // Socket speed limit for this h2 connection: server shrunk to the SNI site.
        let limit = site
            .tcp_sock_speed_limit()
            .shrink_as_smaller(&self.ctx.server_config.tcp_sock_speed_limit);
        let site_io_stats = site.stats().fetch_traffic_stats(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );
        let stream = LimitedStream::local_limited(
            stream,
            limit.shift_millis,
            limit.max_north,
            limit.max_south,
            H2ConnectionCltWrapperStats::new(&self.ctx.server_stats, site_io_stats),
        );

        let mut server_builder = h2::server::Builder::new();
        self.ctx
            .server_config
            .h2
            .apply_to_server_builder(&mut server_builder);

        let mut h2c = tokio::time::timeout(
            self.ctx.server_config.h2.client_handshake_timeout,
            server_builder.handshake(stream),
        )
        .await
        .map_err(|_| anyhow::anyhow!("client h2 handshake timeout"))?
        .map_err(|e| anyhow::anyhow!("client h2 handshake: {e}"))?;

        let stats = Arc::new(H2ConcurrencyStats::default());
        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                clt_r = h2c.accept() => {
                    match clt_r {
                        Some(Ok((clt_req, clt_send_rsp))) => {
                            idle_count = 0;
                            let ctx = Arc::clone(&self.ctx);
                            let hosts = Arc::clone(&self.hosts);
                            let task_guard = stats.add_task();
                            tokio::spawn(async move {
                                stream::transfer(clt_req, clt_send_rsp, ctx, hosts).await;
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
                    if stats.get_alive_task() <= 0 {
                        idle_count += n;
                        if idle_count > self.ctx.server_config.task_idle_max_count {
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
            }
        }
    }
}
