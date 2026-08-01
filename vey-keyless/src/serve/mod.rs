/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

use vey_daemon::listen::{AcceptTcpServer, ListenStats};
use vey_daemon::server::{BaseServer, ClientConnectionInfo, ServerQuitPolicy};
use vey_types::metrics::{MetricTagName, MetricTagValue, NodeName};

use crate::config::server::AnyKeyServerConfig;

mod stats;
pub(crate) use stats::{
    KeyServerAliveTaskGuard, KeyServerDurationRecorder, KeyServerDurationStats,
    KeyServerRequestSnapshot, KeyServerRequestStats, KeyServerSnapshot, KeyServerStats,
};

mod error;
pub(crate) use error::ServerTaskError;

mod task;
use task::{KeylessTask, KeylessTaskContext};
pub(crate) use task::{RequestProcessContext, WrappedKeylessRequest, WrappedKeylessResponse};

mod runtime;
use runtime::KeyServerRuntime;

mod registry;
pub(crate) use registry::{foreach_online as foreach_server, get_names};

mod ops;
pub use ops::{create_all_stopped, spawn_all, spawn_offline_clean, start_all_stopped};
pub(crate) use ops::{get_server, stop_all, wait_all_tasks};

mod cloudflare;
mod plain_tcp_port;
mod plain_tls_port;

use cloudflare::CloudflareServer;
use plain_tcp_port::PlainTcpPort;
use plain_tls_port::PlainTlsPort;

#[derive(Clone)]
pub(crate) enum ServerReloadCommand {
    QuitRuntime,
}

/// A server that can accept client connections and either handle key operations itself,
/// or hand the connection over to another server.
#[async_trait]
pub(crate) trait KeyServer: BaseServer + AcceptTcpServer {
    /// The address this server listens on, or `None` if it only serves chained connections.
    fn listen_addr(&self) -> Option<SocketAddr> {
        None
    }

    fn get_listen_stats(&self) -> Arc<ListenStats>;

    /// Key operation stats, only available on servers that speak a key operation protocol.
    fn get_server_stats(&self) -> Option<Arc<KeyServerStats>> {
        None
    }

    /// Key operation duration stats, only available on servers that speak a key operation
    /// protocol.
    fn get_duration_stats(&self) -> Option<Arc<KeyServerDurationStats>> {
        None
    }

    fn alive_count(&self) -> i32 {
        0
    }

    fn quit_policy(&self) -> &Arc<ServerQuitPolicy>;

    fn add_dynamic_metrics_tag(&self, _name: MetricTagName, _value: MetricTagValue) {}

    /// Handle a connection whose TLS layer has already been terminated by a previous server.
    async fn run_rustls_task(&self, stream: TlsStream<TcpStream>, cc_info: ClientConnectionInfo);
}

trait KeyServerInternal: KeyServer {
    fn _clone_config(&self) -> AnyKeyServerConfig;

    fn _depend_on_server(&self, _name: &NodeName) -> bool {
        false
    }

    /// Re-resolve the `next` server pointer from the runtime registry.
    fn _update_next_server_in_place(&self) {}

    fn _reload(&self, config: AnyKeyServerConfig) -> anyhow::Result<ArcKeyServerInternal>;

    fn _start_runtime(&self, server: ArcKeyServer) -> anyhow::Result<()>;

    fn _abort_runtime(&self);
}

pub(crate) type ArcKeyServer = Arc<dyn KeyServer + Send + Sync>;
type ArcKeyServerInternal = Arc<dyn KeyServerInternal + Send + Sync>;

fn new_server(config: AnyKeyServerConfig) -> anyhow::Result<ArcKeyServerInternal> {
    match config {
        AnyKeyServerConfig::Cloudflare(c) => CloudflareServer::prepare_initial(c),
        AnyKeyServerConfig::PlainTcpPort(c) => PlainTcpPort::prepare_initial(c),
        AnyKeyServerConfig::PlainTlsPort(c) => PlainTlsPort::prepare_initial(c),
    }
}
