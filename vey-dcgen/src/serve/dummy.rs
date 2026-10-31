/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::broadcast;

use vey_daemon::listen::{ListenStats, UdpResponder};
use vey_daemon::server::{BaseServer, ServerReloadCommand};
use vey_types::metrics::NodeName;

use super::stats::ServerStats;
use super::{ArcServer, ArcServerInternal, Server, ServerInternal};
use crate::config::server::{AnyServerConfig, DummyServerConfig, ServerConfig};

const SERVER_TYPE: &str = "DUMMY";

pub(super) struct DummyServer {
    config: DummyServerConfig,
    reload_sender: broadcast::Sender<ServerReloadCommand<()>>,
    listen_stats: Arc<ListenStats>,
    server_stats: Arc<ServerStats>,
    reload_version: usize,
}

impl DummyServer {
    fn new(
        config: DummyServerConfig,
        reload_version: usize,
        listen_stats: Arc<ListenStats>,
        server_stats: Arc<ServerStats>,
    ) -> Self {
        DummyServer {
            config,
            reload_sender: ServerReloadCommand::new_sender(),
            listen_stats,
            server_stats,
            reload_version,
        }
    }

    pub(super) fn prepare_default(name: &NodeName) -> ArcServerInternal {
        let config = DummyServerConfig::new(name);
        let listen_stats = Arc::new(ListenStats::new(name));
        let server_stats = Arc::new(ServerStats::new(name));
        Arc::new(DummyServer::new(config, 0, listen_stats, server_stats))
    }

    pub(super) fn prepare_initial(config: DummyServerConfig) -> anyhow::Result<ArcServerInternal> {
        let listen_stats = Arc::new(ListenStats::new(config.name()));
        let server_stats = Arc::new(ServerStats::new(config.name()));
        Ok(Arc::new(DummyServer::new(
            config,
            1,
            listen_stats,
            server_stats,
        )))
    }

    fn prepare_reload(&self, config: DummyServerConfig) -> DummyServer {
        DummyServer::new(
            config,
            self.reload_version + 1,
            Arc::clone(&self.listen_stats),
            Arc::clone(&self.server_stats),
        )
    }
}

impl ServerInternal for DummyServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::Dummy(self.config.clone())
    }

    fn _reload_with_new_notifier(
        &self,
        config: AnyServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let AnyServerConfig::Dummy(config) = config else {
            return Err(anyhow!("config type mismatch: expect {SERVER_TYPE}"));
        };
        Ok(Arc::new(self.prepare_reload(config)))
    }

    fn _start_runtime(&self, _server: ArcServer) -> anyhow::Result<()> {
        Ok(())
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
    }
}

impl BaseServer for DummyServer {
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
impl Server for DummyServer {
    fn listen_stats(&self) -> Arc<ListenStats> {
        Arc::clone(&self.listen_stats)
    }

    fn server_stats(&self) -> Arc<ServerStats> {
        Arc::clone(&self.server_stats)
    }

    async fn handle_udp_request(
        &self,
        _packet: &[u8],
        _responder: UdpResponder,
        _worker_id: Option<usize>,
    ) {
    }

    #[cfg(unix)]
    async fn handle_unix_request(
        &self,
        _packet: &[u8],
        _responder: &vey_daemon::listen::UnixDatagramResponder,
    ) {
    }
}
