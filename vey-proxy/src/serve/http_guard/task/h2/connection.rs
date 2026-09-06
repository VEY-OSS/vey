/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use h2::Reason;
use log::debug;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_types::route::HostMatch;

use super::CommonTaskContext;
use super::stats::H2ConcurrencyStats;
use super::stream;
use crate::serve::http_guard::HttpHost;
use crate::site::SiteHttpConnGuard;

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
        let stream = self.stream.take().unwrap();
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
        let site_conn: Arc<Mutex<Option<SiteHttpConnGuard>>> = Arc::new(Mutex::new(None));
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
                            let site_conn = Arc::clone(&site_conn);
                            let task_guard = stats.add_task();
                            tokio::spawn(async move {
                                stream::transfer(clt_req, clt_send_rsp, ctx, hosts, site_conn)
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
