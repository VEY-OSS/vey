/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use arc_swap::ArcSwap;
use async_trait::async_trait;
use log::debug;
use openssl::ssl::Ssl;
use slog::Logger;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
#[cfg(feature = "openssl-async-job")]
use tokio::runtime::{Handle, RuntimeFlavor};
use tokio::sync::{Semaphore, broadcast};
use tokio_rustls::server::TlsStream;

use vey_daemon::listen::{AcceptTcpServer, ListenStats};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerQuitPolicy};
use vey_openssl::SslAcceptor;
use vey_types::metrics::{MetricTagMap, MetricTagName, MetricTagValue, NodeName};
use vey_types::net::OpensslServerConfig;

use crate::config::server::cloudflare::CloudflareServerConfig;
use crate::config::server::{AnyKeyServerConfig, KeyServerConfig};
use crate::serve::{
    ArcKeyServer, ArcKeyServerInternal, KeyServer, KeyServerDurationRecorder,
    KeyServerDurationStats, KeyServerInternal, KeyServerRuntime, KeyServerStats, KeylessTask,
    KeylessTaskContext, ServerReloadCommand,
};

/// The parts of a server that are carried over to the new instance on reload, so that the
/// emitted metrics stay continuous.
struct CloudflareServerState {
    server_stats: Arc<KeyServerStats>,
    listen_stats: Arc<ListenStats>,
    duration_recorder: KeyServerDurationRecorder,
    duration_stats: Arc<KeyServerDurationStats>,
    dynamic_metrics_tags: Arc<ArcSwap<MetricTagMap>>,
}

/// A server that speaks the Cloudflare Keyless protocol.
pub(crate) struct CloudflareServer {
    config: Arc<CloudflareServerConfig>,
    server_stats: Arc<KeyServerStats>,
    listen_stats: Arc<ListenStats>,
    tls_server_config: Option<OpensslServerConfig>,
    duration_recorder: KeyServerDurationRecorder,
    duration_stats: Arc<KeyServerDurationStats>,
    quit_policy: Arc<ServerQuitPolicy>,
    reload_sender: broadcast::Sender<ServerReloadCommand>,
    reload_version: usize,
    concurrency_limit: Option<Arc<Semaphore>>,
    task_logger: Option<Logger>,
    request_logger: Option<Logger>,
    dynamic_metrics_tags: Arc<ArcSwap<MetricTagMap>>,
}

impl CloudflareServer {
    fn new(
        config: CloudflareServerConfig,
        state: CloudflareServerState,
        reload_version: usize,
    ) -> anyhow::Result<Self> {
        let CloudflareServerState {
            server_stats,
            listen_stats,
            duration_recorder,
            duration_stats,
            dynamic_metrics_tags,
        } = state;
        let reload_sender = broadcast::Sender::new(16);

        let concurrency_limit = if config.concurrency_limit > 0 {
            Some(Arc::new(Semaphore::new(config.concurrency_limit)))
        } else {
            None
        };

        let tls_server_config = if let Some(builder) = &config.tls_server {
            let server = builder
                .build()
                .context("failed to build tls server config")?;
            Some(server)
        } else {
            None
        };

        let task_logger = config.get_task_logger();
        let request_logger = config.get_request_logger();

        // always update extra metrics tags
        let dynamic_tags = dynamic_metrics_tags.load();
        let dynamic_tags = dynamic_tags.as_ref().clone();
        if let Some(conf) = config.extra_metrics_tags.clone() {
            let mut extra = (*conf).clone();
            extra.extend(dynamic_tags);
            let extra = Arc::new(extra);
            server_stats.set_extra_tags(Some(extra.clone()));
            duration_stats.set_extra_tags(Some(extra));
        } else if !dynamic_tags.is_empty() {
            let extra = Arc::new(dynamic_tags);
            server_stats.set_extra_tags(Some(extra.clone()));
            duration_stats.set_extra_tags(Some(extra));
        }

        Ok(CloudflareServer {
            config: Arc::new(config),
            server_stats,
            listen_stats,
            tls_server_config,
            duration_recorder,
            duration_stats,
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            reload_sender,
            reload_version,
            concurrency_limit,
            task_logger,
            request_logger,
            dynamic_metrics_tags,
        })
    }

    pub(super) fn prepare_initial(
        config: CloudflareServerConfig,
    ) -> anyhow::Result<ArcKeyServerInternal> {
        let (duration_recorder, duration_stats) =
            KeyServerDurationRecorder::new(config.name(), &config.duration_stats);
        let state = CloudflareServerState {
            server_stats: Arc::new(KeyServerStats::new(config.name())),
            listen_stats: Arc::new(ListenStats::new(config.name())),
            duration_recorder,
            duration_stats,
            dynamic_metrics_tags: Arc::new(ArcSwap::new(Default::default())),
        };
        let server = CloudflareServer::new(config, state, 1)?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(&self, config: AnyKeyServerConfig) -> anyhow::Result<CloudflareServer> {
        let AnyKeyServerConfig::Cloudflare(config) = config else {
            return Err(anyhow!(
                "config type mismatch: expect {}, actual {}",
                self.config.r#type(),
                config.r#type()
            ));
        };

        let (duration_recorder, duration_stats) =
            if self.config.duration_stats != config.duration_stats {
                KeyServerDurationRecorder::new(config.name(), &config.duration_stats)
            } else {
                (self.duration_recorder.clone(), self.duration_stats.clone())
            };
        let state = CloudflareServerState {
            server_stats: self.server_stats.clone(),
            listen_stats: self.listen_stats.clone(),
            duration_recorder,
            duration_stats,
            dynamic_metrics_tags: self.dynamic_metrics_tags.clone(),
        };
        CloudflareServer::new(config, state, self.reload_version + 1)
    }

    async fn run_task<R, W>(&self, cc_info: ClientConnectionInfo, clt_r: R, clt_w: W)
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let ctx = KeylessTaskContext {
            server_config: self.config.clone(),
            server_stats: self.server_stats.clone(),
            duration_recorder: self.duration_recorder.clone(),
            cc_info,
            task_logger: self.task_logger.clone(),
            request_logger: self.request_logger.clone(),
            reload_notifier: self.reload_sender.subscribe(),
            concurrency_limit: self.concurrency_limit.clone(),
        };

        let mut task = KeylessTask::new(ctx);

        if vey_daemon::runtime::worker::worker_count() > 0 {
            task.set_allow_dispatch();
        }

        #[cfg(feature = "openssl-async-job")]
        if matches!(
            Handle::current().runtime_flavor(),
            RuntimeFlavor::CurrentThread
        ) {
            task.set_allow_openssl_async_job();
        }

        if self.config.multiplex_queue_depth > 1 {
            task.into_multiplex_running(clt_r, clt_w).await
        } else {
            task.into_simplex_running(clt_r, clt_w).await
        }
    }

    async fn run_openssl_task(
        &self,
        tls_server: &OpensslServerConfig,
        stream: TcpStream,
        cc_info: ClientConnectionInfo,
    ) {
        let Ok(ssl) = Ssl::new(&tls_server.ssl_context) else {
            self.listen_stats.add_dropped();
            return;
        };

        let Ok(ssl_acceptor) = SslAcceptor::new(ssl, stream, tls_server.accept_timeout) else {
            self.listen_stats.add_dropped();
            return;
        };

        match ssl_acceptor.accept().await {
            Ok(ssl_stream) => {
                if ssl_stream.ssl().session_reused() {
                    // Quick ACK is needed with session resumption
                    cc_info.tcp_sock_try_quick_ack();
                }
                let (r, w) = tokio::io::split(ssl_stream);
                self.run_task(cc_info, r, w).await
            }
            Err(e) => {
                self.listen_stats.add_failed();
                debug!(
                    "{} - {} tls error: {e:?}",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
                // TODO record tls failure and add some sec policy
            }
        }
    }
}

impl KeyServerInternal for CloudflareServer {
    fn _clone_config(&self) -> AnyKeyServerConfig {
        AnyKeyServerConfig::Cloudflare(self.config.as_ref().clone())
    }

    fn _reload(&self, config: AnyKeyServerConfig) -> anyhow::Result<ArcKeyServerInternal> {
        let server = self.prepare_reload(config)?;
        Ok(Arc::new(server))
    }

    fn _start_runtime(&self, server: ArcKeyServer) -> anyhow::Result<()> {
        let Some(listen_config) = &self.config.listen else {
            // this server only handles connections sent by other servers
            self.server_stats.set_online();
            self.duration_stats.set_online();
            return Ok(());
        };
        KeyServerRuntime::new(server)
            .into_running(listen_config, &self.reload_sender)
            .map(|_| {
                self.server_stats.set_online();
                self.duration_stats.set_online()
            })
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
        self.server_stats.set_offline();
        self.duration_stats.set_offline();
    }
}

impl BaseServer for CloudflareServer {
    #[inline]
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    #[inline]
    fn r#type(&self) -> &'static str {
        self.config.r#type()
    }

    #[inline]
    fn version(&self) -> usize {
        self.reload_version
    }
}

#[async_trait]
impl AcceptTcpServer for CloudflareServer {
    async fn run_tcp_task(&self, stream: TcpStream, cc_info: ClientConnectionInfo) {
        if let Some(tls_server) = &self.tls_server_config {
            self.run_openssl_task(tls_server, stream, cc_info).await
        } else {
            let (r, w) = stream.into_split();
            self.run_task(cc_info, r, w).await
        }
    }
}

#[async_trait]
impl KeyServer for CloudflareServer {
    fn listen_addr(&self) -> Option<SocketAddr> {
        self.config.listen.as_ref().map(|c| c.address())
    }

    #[inline]
    fn get_listen_stats(&self) -> Arc<ListenStats> {
        self.listen_stats.clone()
    }

    #[inline]
    fn get_server_stats(&self) -> Option<Arc<KeyServerStats>> {
        Some(self.server_stats.clone())
    }

    #[inline]
    fn get_duration_stats(&self) -> Option<Arc<KeyServerDurationStats>> {
        Some(self.duration_stats.clone())
    }

    fn alive_count(&self) -> i32 {
        self.server_stats.get_alive_count()
    }

    #[inline]
    fn quit_policy(&self) -> &Arc<ServerQuitPolicy> {
        &self.quit_policy
    }

    fn add_dynamic_metrics_tag(&self, name: MetricTagName, value: MetricTagValue) {
        let dynamic_tags = self.dynamic_metrics_tags.load();
        let mut dynamic_tags = dynamic_tags.as_ref().clone();
        dynamic_tags.insert(name, value);
        self.dynamic_metrics_tags
            .store(Arc::new(dynamic_tags.clone()));

        match self.server_stats.load_extra_tags() {
            Some(extra) => {
                let mut extra = (*extra).clone();
                extra.extend(dynamic_tags);
                self.server_stats.set_extra_tags(Some(Arc::new(extra)))
            }
            None => self
                .server_stats
                .set_extra_tags(Some(Arc::new(dynamic_tags))),
        }
    }

    async fn run_rustls_task(&self, stream: TlsStream<TcpStream>, cc_info: ClientConnectionInfo) {
        let (r, w) = tokio::io::split(stream);
        self.run_task(cc_info, r, w).await
    }
}
