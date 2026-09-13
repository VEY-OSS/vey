/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use slog::Logger;
use tokio::time::Instant;

use vey_daemon::server::ClientConnectionInfo;
use vey_io_ext::{IdleWheel, OptionalInterval};

use vey_icap_client::reqmod::h1::HttpAdapterErrorResponse;

use super::{HttpGuardServerConfig, HttpGuardServerStats};
use crate::audit::AuditHandle;
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::http_forward::HttpProxyClientResponse;
use crate::module::http_header;
use crate::serve::{ServerIdleChecker, ServerQuitPolicy, ServerTaskNotes};
use crate::site::Site;

#[derive(Clone)]
pub(crate) struct CommonTaskContext {
    pub(crate) server_config: Arc<HttpGuardServerConfig>,
    pub(crate) server_stats: Arc<HttpGuardServerStats>,
    pub(crate) server_quit_policy: Arc<ServerQuitPolicy>,
    pub(crate) idle_wheel: Arc<IdleWheel>,
    pub(crate) escaper: ArcEscaper,
    pub(crate) cc_info: ClientConnectionInfo,
    pub(crate) task_logger: Option<Logger>,
    pub(crate) audit_handle: Option<Arc<AuditHandle>>,
    pub(crate) pinned_site: Option<Arc<Site>>,
}

impl CommonTaskContext {
    #[inline]
    pub(crate) fn client_addr(&self) -> SocketAddr {
        self.cc_info.client_addr()
    }

    #[inline]
    pub(crate) fn client_ip(&self) -> IpAddr {
        self.cc_info.client_ip()
    }

    #[inline]
    pub(crate) fn server_addr(&self) -> SocketAddr {
        self.cc_info.server_addr()
    }

    pub(crate) fn apply_proxy_status_ident(&self, rsp: &mut HttpProxyClientResponse) {
        rsp.apply_proxy_status(
            self.server_config.no_proxy_status,
            self.server_config.server_id.as_ref(),
        );
    }

    pub(crate) fn idle_checker(&self, task_notes: &ServerTaskNotes) -> ServerIdleChecker {
        ServerIdleChecker::new(
            self.idle_wheel.clone(),
            None,
            task_notes
                .site_ctx()
                .and_then(|s| s.tenant().map(|t| t.user().clone())),
            task_notes.task_max_idle_count(self.server_config.task_idle_max_count),
            self.server_quit_policy.clone(),
        )
    }

    pub(crate) fn set_custom_header_for_adaptation_error_reply(
        &self,
        egress_notes: &EgressNotes,
        rsp: &mut HttpAdapterErrorResponse,
    ) {
        if let Some(server_id) = &self.server_config.server_id {
            http_header::set_remote_connection_info(
                &mut rsp.headers,
                server_id,
                egress_notes.bind.ip(),
                egress_notes.tcp_connect_local_addr(),
                egress_notes.tcp_connect_peer_addr(),
                &egress_notes.expire,
            );

            if let Some(egress_info) = &egress_notes.egress {
                http_header::set_dynamic_egress_info(&mut rsp.headers, server_id, egress_info);
            }
        }
    }

    pub(super) fn log_flush_interval(&self) -> Option<Duration> {
        self.task_logger.as_ref()?;
        self.server_config.task_log_flush_interval
    }

    pub(super) fn get_log_interval(&self) -> OptionalInterval {
        self.log_flush_interval()
            .map(|log_interval| {
                let log_interval =
                    tokio::time::interval_at(Instant::now() + log_interval, log_interval);
                OptionalInterval::with(log_interval)
            })
            .unwrap_or_default()
    }
}
