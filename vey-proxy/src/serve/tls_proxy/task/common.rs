/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use slog::Logger;

use vey_daemon::server::ClientConnectionInfo;
use vey_io_ext::IdleWheel;

use crate::config::server::tls_proxy::TlsProxyServerConfig;
use crate::escape::ArcEscaper;
use crate::serve::ServerQuitPolicy;
use crate::serve::tcp_stream::TcpStreamServerStats;

pub(in crate::serve::tls_proxy) struct CommonTaskContext {
    pub(in crate::serve::tls_proxy) server_config: Arc<TlsProxyServerConfig>,
    pub(in crate::serve::tls_proxy) server_stats: Arc<TcpStreamServerStats>,
    pub(in crate::serve::tls_proxy) server_quit_policy: Arc<ServerQuitPolicy>,
    pub(in crate::serve::tls_proxy) idle_wheel: Arc<IdleWheel>,
    pub(in crate::serve::tls_proxy) escaper: ArcEscaper,
    pub(in crate::serve::tls_proxy) cc_info: ClientConnectionInfo,
    pub(in crate::serve::tls_proxy) task_logger: Option<Logger>,
}

impl CommonTaskContext {
    pub(in crate::serve::tls_proxy) fn log_flush_interval(&self) -> Option<Duration> {
        self.task_logger.as_ref()?;
        self.server_config.task_log_flush_interval
    }
}
