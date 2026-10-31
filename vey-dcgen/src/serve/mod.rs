/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use async_trait::async_trait;

use vey_daemon::listen::{ListenStats, RespondUdpServer, UdpResponder};
#[cfg(unix)]
use vey_daemon::listen::{RespondUnixDatagramServer, UnixDatagramResponder};
use vey_daemon::server::{BaseServer, ReloadServer};
use vey_types::metrics::NodeName;

use crate::config::server::AnyServerConfig;

mod registry;
pub(crate) use registry::foreach_online;
use registry::get_or_insert_default;

mod ops;
pub use ops::{spawn_all, spawn_offline_clean};
pub(crate) use ops::{stop_all, wait_all_tasks};

mod stats;
pub(crate) use stats::ServerStats;

mod answer;

mod dummy;
mod udp;

#[cfg(unix)]
mod unix;

#[async_trait]
pub(crate) trait Server: BaseServer {
    fn listen_stats(&self) -> Arc<ListenStats>;
    fn server_stats(&self) -> Arc<ServerStats>;
    fn alive_count(&self) -> i32 {
        self.server_stats().alive_count()
    }

    async fn handle_udp_request(
        &self,
        packet: &[u8],
        responder: UdpResponder,
        worker_id: Option<usize>,
    );

    #[cfg(unix)]
    async fn handle_unix_request(&self, packet: &[u8], responder: &UnixDatagramResponder);
}

trait ServerInternal: Server {
    fn _clone_config(&self) -> AnyServerConfig;

    fn _reload_with_new_notifier(
        &self,
        config: AnyServerConfig,
    ) -> anyhow::Result<ArcServerInternal>;

    fn _start_runtime(&self, server: ArcServer) -> anyhow::Result<()>;
    fn _abort_runtime(&self);
}

pub(crate) type ArcServer = Arc<dyn Server + Send + Sync>;
type ArcServerInternal = Arc<dyn ServerInternal + Send + Sync>;

#[derive(Clone)]
struct WrapArcServer(ArcServer);

impl BaseServer for WrapArcServer {
    fn name(&self) -> &NodeName {
        self.0.name()
    }

    fn r#type(&self) -> &'static str {
        self.0.r#type()
    }

    fn version(&self) -> usize {
        self.0.version()
    }
}

impl ReloadServer for WrapArcServer {
    fn reload(&self) -> Self {
        WrapArcServer(get_or_insert_default(self.name()))
    }
}

impl RespondUdpServer for WrapArcServer {
    async fn handle_udp_request(
        &self,
        packet: &[u8],
        responder: UdpResponder,
        worker_id: Option<usize>,
    ) {
        self.0
            .handle_udp_request(packet, responder, worker_id)
            .await
    }
}

#[cfg(unix)]
impl RespondUnixDatagramServer for WrapArcServer {
    async fn handle_unix_request(&self, packet: &[u8], responder: &UnixDatagramResponder) {
        self.0.handle_unix_request(packet, responder).await
    }
}

pub fn spawn_issuer() -> anyhow::Result<()> {
    crate::issuer::spawn_workers()
}
