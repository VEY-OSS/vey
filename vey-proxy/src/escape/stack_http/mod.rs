/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use slog::Logger;

use vey_daemon::stat::remote::{ArcTcpConnectionTaskRemoteStats, ArcUdpConnectTaskRemoteStats};
use vey_types::collection::{SelectiveVec, SelectiveVecBuilder};
use vey_types::metrics::NodeName;
use vey_types::net::{Host, HttpForwardCapability, UpstreamAddr, WeightedUpstreamAddr};

use super::{
    ArcEscaper, ArcEscaperStats, EgressNotes, Escaper, EscaperExt, EscaperInternal,
    EscaperRegistry, EscaperStats, TlsHttpConnection,
};
use crate::audit::AuditContext;
use crate::auth::UserUpstreamTrafficStatsList;
use crate::config::escaper::stack_http::StackHttpEscaperConfig;
use crate::config::escaper::{AnyEscaperConfig, EscaperConfig};
use crate::escape::proxy_http::ProxyHttpEscaperStats;
use crate::module::ftp_over_http::{
    ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpConnectContext,
    BoxFtpRemoteConnection, DirectFtpConnectContext,
};
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, BoxHttpForwardContext,
    ProxyHttpForwardContext,
};
use crate::module::tcp_connect::{
    NestedTcpConnectResult, TcpConnectError, TcpConnectResult, TcpConnectTaskConf,
    TlsConnectTaskConf,
};
use crate::module::udp_connect::{UdpConnectError, UdpConnectResult, UdpConnectTaskConf};
use crate::module::udp_relay::{ArcUdpRelayTaskRemoteStats, UdpRelaySetupResult, UdpRelayTaskConf};
use crate::serve::ServerTaskNotes;

mod http_connect;
mod http_connect_udp;
mod http_forward;
mod tcp_connect;

pub(super) struct StackHttpEscaper {
    config: Arc<StackHttpEscaperConfig>,
    stats: Arc<ProxyHttpEscaperStats>,
    next: ArcEscaper,
    proxy_nodes: SelectiveVec<WeightedUpstreamAddr>,
    escape_logger: Option<Logger>,
    proxy_http_headers: Arc<[String]>,
}

impl StackHttpEscaper {
    fn new_obj<F>(
        config: StackHttpEscaperConfig,
        stats: Arc<ProxyHttpEscaperStats>,
        mut fetch_escaper: F,
    ) -> anyhow::Result<ArcEscaper>
    where
        F: FnMut(&NodeName) -> ArcEscaper,
    {
        let next = fetch_escaper(&config.next);
        let mut nodes_builder = SelectiveVecBuilder::new();
        for node in &config.proxy_nodes {
            nodes_builder.insert(node.clone());
        }
        let proxy_nodes = nodes_builder
            .build()
            .ok_or_else(|| anyhow!("no next proxy node set"))?;

        let escape_logger = config.get_escape_logger();

        stats.set_extra_tags(config.extra_metrics_tags.clone());

        let proxy_http_headers = Arc::from(config.proxy_http_headers());

        let escaper = StackHttpEscaper {
            config: Arc::new(config),
            stats,
            next,
            proxy_nodes,
            escape_logger,
            proxy_http_headers,
        };

        Ok(Arc::new(escaper))
    }

    pub(super) fn prepare_initial(config: StackHttpEscaperConfig) -> anyhow::Result<ArcEscaper> {
        let stats = Arc::new(ProxyHttpEscaperStats::new(config.name()));
        StackHttpEscaper::new_obj(config, stats, super::registry::get_or_insert_default)
    }

    fn prepare_reload(
        config: StackHttpEscaperConfig,
        stats: Arc<ProxyHttpEscaperStats>,
        registry: &mut EscaperRegistry,
    ) -> anyhow::Result<ArcEscaper> {
        StackHttpEscaper::new_obj(config, stats, |name| registry.get_or_insert_default(name))
    }

    fn get_next_proxy(&self, task_notes: &ServerTaskNotes, target_host: &Host) -> &UpstreamAddr {
        self.select_consistent(
            &self.proxy_nodes,
            self.config.proxy_pick_policy,
            task_notes,
            target_host,
        )
        .inner()
    }

    fn fetch_user_upstream_io_stats(
        &self,
        task_notes: &ServerTaskNotes,
    ) -> UserUpstreamTrafficStatsList {
        task_notes.fetch_upstream_traffic_stats(self.name(), self.stats.share_extra_tags())
    }
}

impl EscaperExt for StackHttpEscaper {}

#[async_trait]
impl Escaper for StackHttpEscaper {
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    fn get_escape_stats(&self) -> Option<ArcEscaperStats> {
        Some(self.stats.clone())
    }

    async fn publish(&self, _data: &str) -> anyhow::Result<()> {
        Err(anyhow!("not implemented"))
    }

    async fn tcp_setup_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        _audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult {
        egress_notes.escaper.clone_from(&self.config.name);
        self.http_connect_new_tcp_connection(task_conf, egress_notes, task_notes, task_stats)
            .await
    }

    async fn tls_setup_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        _audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult {
        egress_notes.escaper.clone_from(&self.config.name);
        self.http_connect_new_tls_connection(task_conf, egress_notes, task_notes, task_stats)
            .await
    }

    async fn tls_setup_http_connection(
        &self,
        escaper: ArcEscaper,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        _audit_ctx: &mut AuditContext,
    ) -> Result<TlsHttpConnection, TcpConnectError> {
        egress_notes.escaper.clone_from(&self.config.name);
        self.open_tls_http_connection(escaper, task_conf, egress_notes, task_notes)
            .await
    }

    async fn udp_setup_connection(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        egress_notes.escaper.clone_from(&self.config.name);
        self.http_upgrade_new_udp_connection(task_conf, egress_notes, task_notes, task_stats)
            .await
    }

    async fn udp_setup_relay(
        &self,
        _task_conf: &UdpRelayTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> UdpRelaySetupResult {
        egress_notes.escaper.clone_from(&self.config.name);
        Err(UdpConnectError::MethodUnavailable)
    }

    fn new_http_forward_context(&self, escaper: ArcEscaper) -> BoxHttpForwardContext {
        let ctx = ProxyHttpForwardContext::new(escaper);
        Box::new(ctx)
    }

    async fn new_ftp_connect_context(
        &self,
        escaper: ArcEscaper,
        task_conf: &TcpConnectTaskConf<'_>,
        _task_notes: &ServerTaskNotes,
    ) -> BoxFtpConnectContext {
        Box::new(DirectFtpConnectContext::new(
            escaper,
            task_conf.upstream.clone(),
        ))
    }
}

#[async_trait]
impl EscaperInternal for StackHttpEscaper {
    fn _resolver(&self) -> &NodeName {
        self.config.resolver()
    }

    fn _depend_on_escaper(&self, name: &NodeName) -> bool {
        self.config.next.eq(name)
    }

    fn _clone_config(&self) -> AnyEscaperConfig {
        let config = &*self.config;
        AnyEscaperConfig::StackHttp(config.clone())
    }

    fn _reload(
        &self,
        config: AnyEscaperConfig,
        registry: &mut EscaperRegistry,
    ) -> anyhow::Result<ArcEscaper> {
        let AnyEscaperConfig::StackHttp(config) = config else {
            return Err(anyhow!("invalid escaper config type"));
        };
        let stats = Arc::clone(&self.stats);
        StackHttpEscaper::prepare_reload(config, stats, registry)
    }

    #[inline]
    fn _local_http_forward_capability(&self) -> HttpForwardCapability {
        self.config.http_forward_capability
    }

    async fn _nested_tcp_connect(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> NestedTcpConnectResult {
        egress_notes.escaper.clone_from(&self.config.name);
        self.http_connect_nested_tcp_connect(task_conf, egress_notes, task_notes)
            .await
    }

    async fn _nested_udp_connect(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> UdpConnectResult {
        egress_notes.escaper.clone_from(&self.config.name);
        self.http_upgrade_nested_udp_connect(task_conf, egress_notes, task_notes)
            .await
    }

    async fn _new_http_forward_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        egress_notes.escaper.clone_from(&self.config.name);
        self.http_forward_new_connection(task_conf, egress_notes, task_notes, task_stats)
            .await
    }

    async fn _new_https_forward_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        egress_notes.escaper.clone_from(&self.config.name);
        self.https_forward_new_connection(task_conf, egress_notes, task_notes, task_stats)
            .await
    }

    async fn _new_ftp_control_connection(
        &self,
        _task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        egress_notes.escaper.clone_from(&self.config.name);
        Err(TcpConnectError::MethodUnavailable)
    }

    async fn _new_ftp_transfer_connection(
        &self,
        _task_conf: &TcpConnectTaskConf<'_>,
        transfer_egress_notes: &mut EgressNotes,
        _control_egress_notes: &EgressNotes,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcFtpTaskRemoteTransferStats,
        _ftp_server: &UpstreamAddr,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        transfer_egress_notes.escaper.clone_from(&self.config.name);
        Err(TcpConnectError::MethodUnavailable)
    }
}
