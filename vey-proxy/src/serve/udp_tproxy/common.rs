/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use slog::Logger;

use vey_daemon::server::ClientConnectionInfo;
use vey_io_ext::IdleWheel;

use crate::config::server::udp_tproxy::UdpTProxyServerConfig;
use crate::escape::ArcEscaper;
use crate::serve::ServerQuitPolicy;
use crate::serve::udp_stream::UdpStreamServerStats;

pub(super) struct CommonTaskContext {
    pub(super) server_config: Arc<UdpTProxyServerConfig>,
    pub(super) server_stats: Arc<UdpStreamServerStats>,
    pub(super) server_quit_policy: Arc<ServerQuitPolicy>,
    pub(super) idle_wheel: Arc<IdleWheel>,
    pub(super) escaper: ArcEscaper,
    pub(super) cc_info: ClientConnectionInfo,
    pub(super) task_logger: Option<Logger>,
}

impl CommonTaskContext {
    #[inline]
    pub(super) fn client_addr(&self) -> SocketAddr {
        self.cc_info.client_addr()
    }

    #[inline]
    pub(super) fn server_addr(&self) -> SocketAddr {
        self.cc_info.server_addr()
    }

    pub(super) fn log_flush_interval(&self) -> Option<Duration> {
        self.task_logger.as_ref()?;
        self.server_config.task_log_flush_interval
    }
}
