/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::anyhow;
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use log::debug;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_rustls::server::TlsStream;

use vey_daemon::listen::{AcceptTcpServer, ListenStats};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerQuitPolicy};
use vey_io_ext::haproxy::{ProxyProtocolV1Reader, ProxyProtocolV2Reader};
use vey_types::metrics::NodeName;
use vey_types::net::ProxyProtocolVersion;

use crate::config::server::plain_tcp_port::PlainTcpPortConfig;
use crate::config::server::{AnyKeyServerConfig, KeyServerConfig};
use crate::serve::{
    ArcKeyServer, ArcKeyServerInternal, KeyServer, KeyServerInternal, KeyServerRuntime,
    ServerReloadCommand,
};

/// A plain TCP port that hands every accepted connection over to the next server.
pub(crate) struct PlainTcpPort {
    config: PlainTcpPortConfig,
    listen_stats: Arc<ListenStats>,
    reload_sender: broadcast::Sender<ServerReloadCommand>,
    next_server: ArcSwapOption<ArcKeyServer>,
    quit_policy: Arc<ServerQuitPolicy>,
    reload_version: usize,
}

impl PlainTcpPort {
    fn new(
        config: PlainTcpPortConfig,
        listen_stats: Arc<ListenStats>,
        reload_version: usize,
    ) -> Self {
        let next_server = crate::serve::registry::get_server(&config.server).map(Arc::new);
        PlainTcpPort {
            config,
            listen_stats,
            reload_sender: broadcast::Sender::new(16),
            next_server: ArcSwapOption::new(next_server),
            quit_policy: Arc::new(ServerQuitPolicy::default()),
            reload_version,
        }
    }

    pub(super) fn prepare_initial(
        config: PlainTcpPortConfig,
    ) -> anyhow::Result<ArcKeyServerInternal> {
        let listen_stats = Arc::new(ListenStats::new(config.name()));
        Ok(Arc::new(PlainTcpPort::new(config, listen_stats, 1)))
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
}

impl KeyServerInternal for PlainTcpPort {
    fn _clone_config(&self) -> AnyKeyServerConfig {
        AnyKeyServerConfig::PlainTcpPort(self.config.clone())
    }

    fn _depend_on_server(&self, name: &NodeName) -> bool {
        self.config.server.eq(name)
    }

    fn _update_next_server_in_place(&self) {
        self.next_server
            .store(crate::serve::registry::get_server(&self.config.server).map(Arc::new));
    }

    fn _reload(&self, config: AnyKeyServerConfig) -> anyhow::Result<ArcKeyServerInternal> {
        let AnyKeyServerConfig::PlainTcpPort(config) = config else {
            return Err(anyhow!(
                "config type mismatch: expect {}, actual {}",
                self.config.r#type(),
                config.r#type()
            ));
        };
        Ok(Arc::new(PlainTcpPort::new(
            config,
            self.listen_stats.clone(),
            self.reload_version + 1,
        )))
    }

    fn _start_runtime(&self, server: ArcKeyServer) -> anyhow::Result<()> {
        KeyServerRuntime::new(server).into_running(&self.config.listen, &self.reload_sender)
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
    }
}

impl BaseServer for PlainTcpPort {
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
impl AcceptTcpServer for PlainTcpPort {
    async fn run_tcp_task(&self, mut stream: TcpStream, mut cc_info: ClientConnectionInfo) {
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

        next_server.run_tcp_task(stream, cc_info).await
    }
}

#[async_trait]
impl KeyServer for PlainTcpPort {
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
