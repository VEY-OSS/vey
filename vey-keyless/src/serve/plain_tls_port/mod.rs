/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use log::debug;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_rustls::{TlsAcceptor, server::TlsStream};

use vey_daemon::listen::{AcceptTcpServer, ListenStats};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerQuitPolicy};
use vey_io_ext::haproxy::{ProxyProtocolV1Reader, ProxyProtocolV2Reader};
use vey_types::metrics::NodeName;
use vey_types::net::{
    OpensslTicketKey, ProxyProtocolVersion, RollingTicketer, RustlsServerConnectionExt,
};

use crate::config::server::plain_tls_port::PlainTlsPortConfig;
use crate::config::server::{AnyKeyServerConfig, KeyServerConfig};
use crate::serve::{
    ArcKeyServer, ArcKeyServerInternal, KeyServer, KeyServerInternal, KeyServerRuntime,
    ServerReloadCommand,
};

/// A TLS port backed by rustls, which hands the decrypted stream over to the next server.
pub(crate) struct PlainTlsPort {
    config: PlainTlsPortConfig,
    listen_stats: Arc<ListenStats>,
    tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
    tls_acceptor: TlsAcceptor,
    tls_accept_timeout: Duration,
    reload_sender: broadcast::Sender<ServerReloadCommand>,
    next_server: ArcSwapOption<ArcKeyServer>,
    quit_policy: Arc<ServerQuitPolicy>,
    reload_version: usize,
}

impl PlainTlsPort {
    fn new(
        config: PlainTlsPortConfig,
        listen_stats: Arc<ListenStats>,
        tls_rolling_ticketer: Option<Arc<RollingTicketer<OpensslTicketKey>>>,
        reload_version: usize,
    ) -> anyhow::Result<Self> {
        let Some(builder) = &config.server_tls_config else {
            return Err(anyhow!("no tls server config set"));
        };
        let tls_server_config = builder
            .build_with_ticketer(tls_rolling_ticketer.clone())
            .context("failed to build tls server config")?;
        let next_server = crate::serve::registry::get_server(&config.server).map(Arc::new);

        Ok(PlainTlsPort {
            config,
            listen_stats,
            tls_rolling_ticketer,
            tls_acceptor: TlsAcceptor::from(tls_server_config.driver),
            tls_accept_timeout: tls_server_config.accept_timeout,
            reload_sender: broadcast::Sender::new(16),
            next_server: ArcSwapOption::new(next_server),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            reload_version,
        })
    }

    pub(super) fn prepare_initial(
        config: PlainTlsPortConfig,
    ) -> anyhow::Result<ArcKeyServerInternal> {
        let listen_stats = Arc::new(ListenStats::new(config.name()));

        let tls_rolling_ticketer = if let Some(c) = &config.tls_ticketer {
            let ticketer = c
                .build_and_spawn_updater()
                .context("failed to create tls rolling ticketer")?;
            Some(ticketer)
        } else {
            None
        };

        let server = PlainTlsPort::new(config, listen_stats, tls_rolling_ticketer, 1)?;
        Ok(Arc::new(server))
    }

    fn take_next_server(&self) -> Option<ArcKeyServer> {
        match self.next_server.load_full() {
            Some(next) => Some(next.as_ref().clone()),
            None => {
                self.listen_stats.add_dropped();
                debug!(
                    "server {}: no next server {} found, close the connection",
                    self.config.name(),
                    self.config.server
                );
                None
            }
        }
    }

    async fn run_task(&self, mut stream: TcpStream, mut cc_info: ClientConnectionInfo) {
        let Some(next_server) = self.take_next_server() else {
            return;
        };

        match self.config.proxy_protocol {
            Some(ProxyProtocolVersion::V1) => {
                let mut parser =
                    ProxyProtocolV1Reader::new(self.config.proxy_protocol_read_timeout);
                match parser.read_proxy_protocol_v1_for_tcp(&mut stream).await {
                    Ok(Some(a)) => cc_info.set_proxy_addr(a),
                    Ok(None) => {}
                    Err(e) => {
                        self.listen_stats.add_by_proxy_protocol_error(e);
                        return;
                    }
                }
            }
            Some(ProxyProtocolVersion::V2) => {
                let mut parser =
                    ProxyProtocolV2Reader::new(self.config.proxy_protocol_read_timeout);
                match parser.read_proxy_protocol_v2_for_tcp(&mut stream).await {
                    Ok(Some(a)) => cc_info.set_proxy_addr(a),
                    Ok(None) => {}
                    Err(e) => {
                        self.listen_stats.add_by_proxy_protocol_error(e);
                        return;
                    }
                }
            }
            None => {}
        }

        match tokio::time::timeout(self.tls_accept_timeout, self.tls_acceptor.accept(stream)).await
        {
            Ok(Ok(tls_stream)) => {
                if tls_stream.get_ref().1.session_reused() {
                    // Quick ACK is needed with session resumption
                    cc_info.tcp_sock_try_quick_ack();
                }
                next_server.run_rustls_task(tls_stream, cc_info).await
            }
            Ok(Err(e)) => {
                self.listen_stats.add_failed();
                debug!(
                    "{} - {} tls error: {e:?}",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
                // TODO record tls failure and add some sec policy
            }
            Err(_) => {
                self.listen_stats.add_timeout();
                debug!(
                    "{} - {} tls timeout",
                    cc_info.sock_local_addr(),
                    cc_info.sock_peer_addr()
                );
            }
        }
    }
}

impl KeyServerInternal for PlainTlsPort {
    fn _clone_config(&self) -> AnyKeyServerConfig {
        AnyKeyServerConfig::PlainTlsPort(self.config.clone())
    }

    fn _depend_on_server(&self, name: &NodeName) -> bool {
        self.config.server.eq(name)
    }

    fn _update_next_server_in_place(&self) {
        self.next_server
            .store(crate::serve::registry::get_server(&self.config.server).map(Arc::new));
    }

    fn _reload(&self, config: AnyKeyServerConfig) -> anyhow::Result<ArcKeyServerInternal> {
        let AnyKeyServerConfig::PlainTlsPort(config) = config else {
            return Err(anyhow!(
                "config type mismatch: expect {}, actual {}",
                self.config.r#type(),
                config.r#type()
            ));
        };

        let tls_rolling_ticketer = if self.config.tls_ticketer.eq(&config.tls_ticketer) {
            self.tls_rolling_ticketer.clone()
        } else if let Some(c) = &config.tls_ticketer {
            let ticketer = c
                .build_and_spawn_updater()
                .context("failed to create tls rolling ticketer")?;
            Some(ticketer)
        } else {
            None
        };

        let server = PlainTlsPort::new(
            config,
            self.listen_stats.clone(),
            tls_rolling_ticketer,
            self.reload_version + 1,
        )?;
        Ok(Arc::new(server))
    }

    fn _start_runtime(&self, server: ArcKeyServer) -> anyhow::Result<()> {
        KeyServerRuntime::new(server).into_running(&self.config.listen, &self.reload_sender)
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
    }
}

impl BaseServer for PlainTlsPort {
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
impl AcceptTcpServer for PlainTlsPort {
    async fn run_tcp_task(&self, stream: TcpStream, cc_info: ClientConnectionInfo) {
        self.run_task(stream, cc_info).await
    }
}

#[async_trait]
impl KeyServer for PlainTlsPort {
    fn listen_addr(&self) -> Option<SocketAddr> {
        Some(self.config.listen.address())
    }

    #[inline]
    fn get_listen_stats(&self) -> Arc<ListenStats> {
        self.listen_stats.clone()
    }

    #[inline]
    fn quit_policy(&self) -> &Arc<ServerQuitPolicy> {
        &self.quit_policy
    }

    async fn run_rustls_task(&self, _stream: TlsStream<TcpStream>, _cc_info: ClientConnectionInfo) {
    }
}
