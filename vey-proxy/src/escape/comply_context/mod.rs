/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_types::metrics::NodeName;
use vey_types::net::UpstreamAddr;

use super::{ArcEscaper, Escaper, EscaperInternal, EscaperRegistry, RouteEscaperStats};
use crate::audit::AuditContext;
use crate::config::escaper::comply_context::ComplyContextEscaperConfig;
use crate::config::escaper::{AnyEscaperConfig, EscaperConfig};
use crate::module::ftp_over_http::{
    ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpConnectContext,
    BoxFtpRemoteConnection,
};
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, BoxHttpForwardContext,
    RouteHttpForwardContext,
};
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectResult, TcpConnectTaskConf, TcpConnectTaskNotes, TlsConnectTaskConf,
};
use crate::module::udp_connect::{
    ArcUdpConnectTaskRemoteStats, UdpConnectResult, UdpConnectTaskConf, UdpConnectTaskNotes,
};
use crate::module::udp_relay::{
    ArcUdpRelayTaskRemoteStats, UdpRelaySetupResult, UdpRelayTaskConf, UdpRelayTaskNotes,
};
use crate::serve::ServerTaskNotes;

pub(super) struct ComplyContextEscaper {
    config: ComplyContextEscaperConfig,
    stats: Arc<RouteEscaperStats>,
    next: ArcEscaper,
}

impl ComplyContextEscaper {
    fn new_obj<F>(
        config: ComplyContextEscaperConfig,
        stats: Arc<RouteEscaperStats>,
        mut fetch_escaper: F,
    ) -> anyhow::Result<ArcEscaper>
    where
        F: FnMut(&NodeName) -> ArcEscaper,
    {
        let next = fetch_escaper(&config.next);

        let escaper = ComplyContextEscaper {
            config,
            stats,
            next,
        };
        Ok(Arc::new(escaper))
    }

    pub(super) fn prepare_initial(
        config: ComplyContextEscaperConfig,
    ) -> anyhow::Result<ArcEscaper> {
        let stats = Arc::new(RouteEscaperStats::new(config.name()));
        ComplyContextEscaper::new_obj(config, stats, super::registry::get_or_insert_default)
    }

    fn prepare_reload(
        config: AnyEscaperConfig,
        stats: Arc<RouteEscaperStats>,
        registry: &mut EscaperRegistry,
    ) -> anyhow::Result<ArcEscaper> {
        if let AnyEscaperConfig::ComplyContext(config) = config {
            ComplyContextEscaper::new_obj(config, stats, |name| {
                registry.get_or_insert_default(name)
            })
        } else {
            Err(anyhow!("invalid escaper config type"))
        }
    }
}

#[async_trait]
impl Escaper for ComplyContextEscaper {
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    fn ref_route_stats(&self) -> Option<&Arc<RouteEscaperStats>> {
        Some(&self.stats)
    }

    async fn publish(&self, _data: &str) -> anyhow::Result<()> {
        Err(anyhow!("not implemented"))
    }

    async fn tcp_setup_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        tcp_notes: &mut TcpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult {
        tcp_notes.escaper.clone_from(&self.config.name);
        self._update_egress_path(task_notes);
        self.stats.add_request_passed();
        self.next
            .tcp_setup_connection(task_conf, tcp_notes, task_notes, task_stats, audit_ctx)
            .await
    }

    async fn tls_setup_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        tcp_notes: &mut TcpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult {
        tcp_notes.escaper.clone_from(&self.config.name);
        self._update_egress_path(task_notes);
        self.stats.add_request_passed();
        self.next
            .tls_setup_connection(task_conf, tcp_notes, task_notes, task_stats, audit_ctx)
            .await
    }

    async fn udp_setup_connection(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        udp_notes: &mut UdpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        udp_notes.escaper.clone_from(&self.config.name);
        self._update_egress_path(task_notes);
        self.stats.add_request_passed();
        self.next
            .udp_setup_connection(task_conf, udp_notes, task_notes, task_stats)
            .await
    }

    async fn udp_setup_relay(
        &self,
        task_conf: &UdpRelayTaskConf<'_>,
        udp_notes: &mut UdpRelayTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> UdpRelaySetupResult {
        udp_notes.escaper.clone_from(&self.config.name);
        self._update_egress_path(task_notes);
        self.stats.add_request_passed();
        self.next
            .udp_setup_relay(task_conf, udp_notes, task_notes, task_stats)
            .await
    }

    fn new_http_forward_context(&self, escaper: ArcEscaper) -> BoxHttpForwardContext {
        let ctx = RouteHttpForwardContext::new(escaper);
        Box::new(ctx)
    }

    async fn new_ftp_connect_context(
        &self,
        escaper: ArcEscaper,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
    ) -> BoxFtpConnectContext {
        self._update_egress_path(task_notes);
        self.stats.add_request_passed();
        self.next
            .new_ftp_connect_context(Arc::clone(&escaper), task_conf, task_notes)
            .await
    }
}

#[async_trait]
impl EscaperInternal for ComplyContextEscaper {
    fn _resolver(&self) -> &NodeName {
        Default::default()
    }

    fn _auditor(&self) -> Option<&NodeName> {
        None
    }

    fn _depend_on_escaper(&self, name: &NodeName) -> bool {
        self.config.next.eq(name)
    }

    fn _clone_config(&self) -> AnyEscaperConfig {
        AnyEscaperConfig::ComplyContext(self.config.clone())
    }

    fn _reload(
        &self,
        config: AnyEscaperConfig,
        registry: &mut EscaperRegistry,
    ) -> anyhow::Result<ArcEscaper> {
        let stats = Arc::clone(&self.stats);
        ComplyContextEscaper::prepare_reload(config, stats, registry)
    }

    async fn _check_out_next_escaper(
        &self,
        _task_notes: &ServerTaskNotes,
        _upstream: &UpstreamAddr,
    ) -> Option<ArcEscaper> {
        self.stats.add_request_passed();
        Some(self.next.clone())
    }

    fn _update_egress_path(&self, task_notes: &ServerTaskNotes) {
        let Some(egress_path) = &task_notes.egress_path_selection else {
            return;
        };
        let egress_context = egress_path.context_kv();

        for config in &self.config.set_egress_upstream {
            if let Some(egress_upstream) = config.build_with_context(egress_context) {
                egress_path.set_upstream(config.escaper.clone(), egress_upstream);
            }
        }

        for config in &self.config.set_egress_index {
            if !config.number_index_key.is_empty()
                && let Some(v) = egress_context.get(&config.number_index_key)
                && let Ok(id) = usize::from_str(v)
            {
                egress_path.set_number_id(config.escaper.clone(), id);
            }

            if !config.string_index_key.is_empty()
                && let Some(v) = egress_context.get(&config.string_index_key)
            {
                egress_path.set_string_id(config.escaper.clone(), v.to_string());
            }
        }
    }

    async fn _new_http_forward_connection(
        &self,
        _task_conf: &TcpConnectTaskConf<'_>,
        tcp_notes: &mut TcpConnectTaskNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        tcp_notes.escaper.clone_from(&self.config.name);
        Err(TcpConnectError::MethodUnavailable)
    }

    async fn _new_https_forward_connection(
        &self,
        _task_conf: &TlsConnectTaskConf<'_>,
        tcp_notes: &mut TcpConnectTaskNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        tcp_notes.escaper.clone_from(&self.config.name);
        Err(TcpConnectError::MethodUnavailable)
    }

    async fn _new_ftp_control_connection(
        &self,
        _task_conf: &TcpConnectTaskConf<'_>,
        tcp_notes: &mut TcpConnectTaskNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        tcp_notes.escaper.clone_from(&self.config.name);
        Err(TcpConnectError::MethodUnavailable)
    }

    async fn _new_ftp_transfer_connection(
        &self,
        _task_conf: &TcpConnectTaskConf<'_>,
        transfer_tcp_notes: &mut TcpConnectTaskNotes,
        _control_tcp_notes: &TcpConnectTaskNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcFtpTaskRemoteTransferStats,
        _ftp_server: &UpstreamAddr,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        transfer_tcp_notes.escaper.clone_from(&self.config.name);
        Err(TcpConnectError::MethodUnavailable)
    }
}
