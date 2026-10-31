/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::broadcast;

use vey_daemon::listen::{ListenStats, RespondUdpRuntime, UdpResponder};
use vey_daemon::server::{BaseServer, ServerReloadCommand};
use vey_types::metrics::NodeName;

use super::answer;
use super::stats::ServerStats;
use super::{ArcServer, ArcServerInternal, Server, ServerInternal, WrapArcServer};
use crate::config::server::{AnyServerConfig, ServerConfig, UdpServerConfig};

const SERVER_TYPE: &str = "UDP";
const UDP_PACKET_MAX_SIZE: u16 = 16384;

pub(super) struct UdpServer {
    config: UdpServerConfig,
    reload_sender: broadcast::Sender<ServerReloadCommand<()>>,
    listen_stats: Arc<ListenStats>,
    server_stats: Arc<ServerStats>,
    reload_version: usize,
}

impl UdpServer {
    fn new(
        config: UdpServerConfig,
        reload_version: usize,
        listen_stats: Arc<ListenStats>,
        server_stats: Arc<ServerStats>,
    ) -> Self {
        UdpServer {
            config,
            reload_sender: ServerReloadCommand::new_sender(),
            listen_stats,
            server_stats,
            reload_version,
        }
    }

    pub(super) fn prepare_initial(config: UdpServerConfig) -> anyhow::Result<ArcServerInternal> {
        let listen_stats = Arc::new(ListenStats::new(config.name()));
        let server_stats = Arc::new(ServerStats::new(config.name()));
        Ok(Arc::new(UdpServer::new(
            config,
            1,
            listen_stats,
            server_stats,
        )))
    }

    fn prepare_reload(&self, config: UdpServerConfig) -> UdpServer {
        UdpServer::new(
            config,
            self.reload_version + 1,
            Arc::clone(&self.listen_stats),
            Arc::clone(&self.server_stats),
        )
    }
}

impl ServerInternal for UdpServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::Udp(self.config.clone())
    }

    fn _reload_with_new_notifier(
        &self,
        config: AnyServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let AnyServerConfig::Udp(config) = config else {
            return Err(anyhow!("config type mismatch: expect {SERVER_TYPE}"));
        };
        Ok(Arc::new(self.prepare_reload(config)))
    }

    fn _start_runtime(&self, server: ArcServer) -> anyhow::Result<()> {
        let mut runtime = RespondUdpRuntime::new(
            WrapArcServer(server),
            self.config.listen.clone(),
            Arc::clone(&self.listen_stats),
            UDP_PACKET_MAX_SIZE,
        );
        if let Err(e) = runtime.run_all_instances(true, &self.reload_sender) {
            let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
            return Err(e);
        }
        Ok(())
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
    }
}

impl BaseServer for UdpServer {
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    fn r#type(&self) -> &'static str {
        SERVER_TYPE
    }

    fn version(&self) -> usize {
        self.reload_version
    }
}

#[async_trait]
impl Server for UdpServer {
    fn listen_stats(&self) -> Arc<ListenStats> {
        Arc::clone(&self.listen_stats)
    }

    fn server_stats(&self) -> Arc<ServerStats> {
        Arc::clone(&self.server_stats)
    }

    async fn handle_udp_request(
        &self,
        packet: &[u8],
        responder: UdpResponder,
        _worker_id: Option<usize>,
    ) {
        let peer = responder.client_addr().to_string();
        answer::answer(&self.server_stats, packet, &peer, move |buf| {
            let responder = responder.clone();
            async move { responder.send(&buf).await }
        })
        .await;
    }

    #[cfg(unix)]
    async fn handle_unix_request(
        &self,
        _packet: &[u8],
        _responder: &vey_daemon::listen::UnixDatagramResponder,
    ) {
    }
}
