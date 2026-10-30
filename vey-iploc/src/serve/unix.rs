/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::sync::broadcast;

use vey_daemon::listen::{
    ListenStats, RespondUnixDatagramRuntime, UdpResponder, UnixDatagramResponder,
};
use vey_daemon::server::{BaseServer, ServerReloadCommand};
use vey_types::metrics::NodeName;

use super::query;
use super::stats::ServerStats;
use super::{ArcServer, ArcServerInternal, Server, ServerInternal, WrapArcServer};
use crate::config::server::{AnyServerConfig, ServerConfig, UnixServerConfig};

const SERVER_TYPE: &str = "UNIX";

pub(super) struct UnixServer {
    config: UnixServerConfig,
    reload_sender: broadcast::Sender<ServerReloadCommand<()>>,
    listen_stats: Arc<ListenStats>,
    server_stats: Arc<ServerStats>,
    reload_version: usize,
}

impl UnixServer {
    fn new(
        config: UnixServerConfig,
        reload_version: usize,
        listen_stats: Arc<ListenStats>,
        server_stats: Arc<ServerStats>,
    ) -> Self {
        UnixServer {
            config,
            reload_sender: ServerReloadCommand::new_sender(),
            listen_stats,
            server_stats,
            reload_version,
        }
    }

    pub(super) fn prepare_initial(config: UnixServerConfig) -> anyhow::Result<ArcServerInternal> {
        let listen_stats = Arc::new(ListenStats::new(config.name()));
        let server_stats = Arc::new(ServerStats::new(config.name()));
        Ok(Arc::new(UnixServer::new(
            config,
            1,
            listen_stats,
            server_stats,
        )))
    }

    fn prepare_reload(&self, config: UnixServerConfig) -> UnixServer {
        UnixServer::new(
            config,
            self.reload_version + 1,
            Arc::clone(&self.listen_stats),
            Arc::clone(&self.server_stats),
        )
    }
}

impl ServerInternal for UnixServer {
    fn _clone_config(&self) -> AnyServerConfig {
        AnyServerConfig::Unix(self.config.clone())
    }

    fn _reload_with_new_notifier(
        &self,
        config: AnyServerConfig,
    ) -> anyhow::Result<ArcServerInternal> {
        let AnyServerConfig::Unix(config) = config else {
            return Err(anyhow!("config type mismatch: expect {SERVER_TYPE}"));
        };
        Ok(Arc::new(self.prepare_reload(config)))
    }

    fn _start_runtime(&self, server: ArcServer) -> anyhow::Result<()> {
        let runtime = RespondUnixDatagramRuntime::new(
            WrapArcServer(server),
            self.config.listen.clone(),
            Arc::clone(&self.listen_stats),
        );
        if let Err(e) = runtime.spawn(&self.reload_sender) {
            let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
            return Err(e);
        }
        Ok(())
    }

    fn _abort_runtime(&self) {
        let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
    }
}

impl BaseServer for UnixServer {
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
impl Server for UnixServer {
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

    async fn handle_unix_request(&self, packet: &[u8], responder: &UnixDatagramResponder) {
        let _alive = self.server_stats.enter_alive();
        let Some(buf) = query::build_response(packet, &self.server_stats) else {
            return;
        };
        if responder.send(&buf).await.is_err() {
            self.server_stats.add_response_fail();
        }
    }
}
