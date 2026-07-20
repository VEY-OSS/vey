/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use slog::Logger;
use tokio::time::Instant;

use vey_daemon::server::ClientConnectionInfo;
use vey_icap_client::reqmod::h1::HttpAdapterErrorResponse;
use vey_io_ext::{IdleWheel, OptionalInterval};
use vey_types::acl::AclAction;
use vey_types::acl_set::AclDstHostRuleSet;
use vey_types::net::{OpensslClientConfig, UpstreamAddr};

use super::{HttpProxyServerConfig, HttpProxyServerStats};
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::http_forward::HttpProxyClientResponse;
use crate::module::http_header;
use crate::serve::{ServerIdleChecker, ServerQuitPolicy, ServerTaskNotes};

#[derive(Clone)]
pub(crate) struct CommonTaskContext {
    pub(crate) server_config: Arc<HttpProxyServerConfig>,
    pub(crate) server_stats: Arc<HttpProxyServerStats>,
    pub(crate) server_quit_policy: Arc<ServerQuitPolicy>,
    pub(crate) idle_wheel: Arc<IdleWheel>,
    pub(crate) escaper: ArcEscaper,
    pub(crate) cc_info: ClientConnectionInfo,
    pub(crate) tls_client_config: Arc<OpensslClientConfig>,
    pub(crate) task_logger: Option<Logger>,

    pub(crate) dst_host_filter: Option<Arc<AclDstHostRuleSet>>,
}

impl CommonTaskContext {
    #[inline]
    pub(crate) fn server_addr(&self) -> SocketAddr {
        self.cc_info.server_addr()
    }

    #[inline]
    pub(crate) fn client_addr(&self) -> SocketAddr {
        self.cc_info.client_addr()
    }

    pub(crate) fn idle_checker(&self, task_notes: &ServerTaskNotes) -> ServerIdleChecker {
        ServerIdleChecker::new(
            self.idle_wheel.clone(),
            task_notes.user_ctx().map(|c| c.user().clone()),
            self.server_config.task_idle_max_count,
            self.server_quit_policy.clone(),
        )
    }

    pub(crate) fn check_upstream(&self, upstream: &UpstreamAddr) -> AclAction {
        let mut default_action = if upstream.is_empty() {
            AclAction::Forbid
        } else {
            AclAction::Permit
        };

        if let Some(filter) = &self.server_config.dst_port_filter {
            let port = upstream.port();
            let (found, action) = filter.check_port(&port);
            if found && action.forbid_early() {
                return action;
            };
            default_action = default_action.restrict(action);
        }

        if let Some(filter) = &self.dst_host_filter {
            let (found, action) = filter.check(upstream.host());
            if found && action.forbid_early() {
                return action;
            }
            default_action = default_action.restrict(action);
        }

        default_action
    }

    pub(crate) fn set_custom_header_for_tcp_local_reply(
        &self,
        egress_notes: &EgressNotes,
        rsp: &mut HttpProxyClientResponse,
    ) {
        if let Some(server_id) = &self.server_config.server_id {
            let line = http_header::remote_connection_info(
                server_id,
                egress_notes.bind.ip(),
                egress_notes.tcp_connect_local_addr(),
                egress_notes.tcp_connect_peer_addr(),
                &egress_notes.expire,
            );
            rsp.add_extra_header(line);

            if let Some(egress_info) = &egress_notes.egress {
                let line = http_header::dynamic_egress_info(server_id, egress_info);
                rsp.add_extra_header(line);
            }
        }

        if self.server_config.echo_chained_info {
            if let Some(addr) = egress_notes.final_addr.target_addr {
                rsp.set_upstream_addr(addr);
            }

            if let Some(addr) = egress_notes.final_addr.outgoing_addr {
                rsp.set_outgoing_ip(addr.ip());
            }
        }
    }

    pub(crate) fn set_custom_header_for_udp_local_reply(
        &self,
        egress_notes: &EgressNotes,
        rsp: &mut HttpProxyClientResponse,
    ) {
        if let Some(server_id) = &self.server_config.server_id {
            let line = http_header::remote_connection_info(
                server_id,
                egress_notes.bind.ip(),
                egress_notes.udp_connect_local_addr(),
                egress_notes.udp_connect_peer_addr(),
                &egress_notes.expire,
            );
            rsp.add_extra_header(line);

            if let Some(egress_info) = &egress_notes.egress {
                let line = http_header::dynamic_egress_info(server_id, egress_info);
                rsp.add_extra_header(line);
            }
        }

        if self.server_config.echo_chained_info {
            if let Some(addr) = egress_notes.final_addr.target_addr {
                rsp.set_upstream_addr(addr);
            }

            if let Some(addr) = egress_notes.final_addr.outgoing_addr {
                rsp.set_outgoing_ip(addr.ip());
            }
        }
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

        if self.server_config.echo_chained_info {
            if let Some(addr) = egress_notes.final_addr.target_addr {
                http_header::set_upstream_addr(&mut rsp.headers, addr);
            }

            if let Some(addr) = egress_notes.final_addr.outgoing_addr {
                http_header::set_outgoing_ip(&mut rsp.headers, addr);
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
