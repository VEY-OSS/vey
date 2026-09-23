/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;

use vey_daemon::stat::remote::{
    ArcTcpConnectionTaskRemoteStats, ArcUdpConnectTaskRemoteStats,
    TcpConnectionTaskRemoteStatsWrapper,
};
use vey_io_ext::{LimitedReader, LimitedWriter};
use vey_types::collection::{SelectiveItem, SelectivePickPolicy, SelectiveVec};
use vey_types::metrics::NodeName;
use vey_types::net::{Host, HttpForwardCapability, UpstreamAddr};

use crate::audit::AuditContext;
use crate::auth::UserUpstreamTrafficStatsList;
use crate::config::escaper::AnyEscaperConfig;
use crate::module::ftp_over_http::{
    ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpConnectContext,
    BoxFtpRemoteConnection,
};
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, BoxHttpForwardContext,
    HttpForwardTaskRemoteWrapperStats, TlsHttpForwardReader, TlsHttpForwardWriter,
};
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectResult, TcpConnectTaskConf, TcpConnection, TlsConnectTaskConf,
};
use crate::module::udp_connect::{UdpConnectResult, UdpConnectTaskConf};
use crate::module::udp_relay::{ArcUdpRelayTaskRemoteStats, UdpRelaySetupResult, UdpRelayTaskConf};
use crate::serve::ServerTaskNotes;

mod registry;
use registry::EscaperRegistry;

mod peer_health;
use peer_health::PeerHealthTable;
pub(crate) use registry::{foreach as foreach_escaper, get_names, get_or_insert_default};

mod stats;
pub(crate) use stats::{
    ArcEscaperStats, EscaperForbiddenSnapshot, EscaperForbiddenStats, EscaperInterfaceStats,
    EscaperInternalStats, EscaperStats, EscaperTcpConnectSnapshot, EscaperTcpStats,
    EscaperTlsSnapshot, EscaperTlsStats, EscaperUdpStats, RouteEscaperSnapshot, RouteEscaperStats,
};

mod egress_path;
pub(crate) use egress_path::EgressPathSelection;

mod egress_notes;
pub(crate) use egress_notes::{EgressNotes, EgressSocketType};

mod comply_audit;
mod comply_context;
mod direct_fixed;
mod direct_float;
mod divert_tcp;
mod dummy_deny;
mod proxy_float;
mod proxy_http;
mod proxy_https;
mod proxy_socks5;
mod proxy_socks5s;
mod route_client;
mod route_failover;
mod route_geoip;
mod route_mapping;
mod route_query;
mod route_resolved;
mod route_select;
mod route_upstream;
mod trick_float;

mod ops;
pub use ops::load_all;
pub(crate) use ops::{
    get_escaper, reload, update_dependency_to_auditor, update_dependency_to_resolver,
};

/// Functions in this trait should only be called from registry module,
/// as Escaper and its reload notifier should be locked together.
/// If not locked, there may be reload notify during getting Escaper and
/// its notifier, which will lead to missing of the notification.
#[async_trait]
pub(crate) trait EscaperInternal {
    fn _resolver(&self) -> &NodeName;
    fn _auditor(&self) -> Option<&NodeName> {
        None
    }
    fn _depend_on_escaper(&self, name: &NodeName) -> bool;

    fn _clone_config(&self) -> AnyEscaperConfig;

    fn _reload(
        &self,
        config: AnyEscaperConfig,
        registry: &mut EscaperRegistry,
    ) -> anyhow::Result<ArcEscaper>;

    fn _clean_to_offline(&self) {}

    fn _local_http_forward_capability(&self) -> HttpForwardCapability {
        HttpForwardCapability::default()
    }

    async fn _check_out_next_escaper(
        &self,
        _task_notes: &ServerTaskNotes,
        _upstream: &UpstreamAddr,
    ) -> Option<ArcEscaper> {
        None
    }
    fn _update_audit_context(&self, _audit_ctx: &mut AuditContext) {}
    fn _update_egress_path(&self, _task_notes: &ServerTaskNotes) {}

    #[allow(unused)]
    async fn _nested_tcp_connect(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult;

    #[allow(unused)]
    async fn _nested_udp_connect(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> UdpConnectResult;

    async fn _new_http_forward_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError>;

    async fn _new_https_forward_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError>;

    async fn _new_ftp_control_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError>;
    async fn _new_ftp_transfer_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        transfer_egress_notes: &mut EgressNotes,
        control_egress_notes: &EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteTransferStats,
        ftp_server: &UpstreamAddr,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError>;

    fn _trick_float_weight(&self) -> u8 {
        0
    }
}

#[async_trait]
pub(crate) trait Escaper: EscaperInternal {
    fn name(&self) -> &NodeName;
    fn get_escape_stats(&self) -> Option<ArcEscaperStats> {
        None
    }
    fn ref_route_stats(&self) -> Option<&Arc<RouteEscaperStats>> {
        None
    }

    fn prepare_reused_http_forward_connection(
        &self,
        mut connection: BoxHttpForwardConnection,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        is_tls: bool,
    ) -> BoxHttpForwardConnection {
        let all_user_stats = if let Some(escaper_stats) = self.get_escape_stats() {
            if is_tls {
                escaper_stats.add_https_forward_request_attempted();
            } else {
                escaper_stats.add_http_forward_request_attempted();
            }
            task_notes.fetch_upstream_traffic_stats(
                escaper_stats.name(),
                escaper_stats.share_extra_tags(),
            )
        } else {
            Default::default()
        };
        connection
            .0
            .update_stats(&task_stats, all_user_stats.clone());
        connection.1.update_stats(&task_stats, all_user_stats);
        connection
    }

    fn fetch_user_upstream_io_stats(
        &self,
        task_notes: &ServerTaskNotes,
    ) -> UserUpstreamTrafficStatsList {
        match self.get_escape_stats() {
            Some(stats) => {
                task_notes.fetch_upstream_traffic_stats(self.name(), stats.share_extra_tags())
            }
            None => Default::default(),
        }
    }

    /// Attach task + user stats on a stream from [`Self::tls_connect`].
    ///
    /// Use after ALPN selects HTTP/2. Escaper IO stays on the TCP layer.
    fn tls_connection_with_task_stats(
        &self,
        stream: TcpConnection,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> TcpConnection {
        let mut wrapper_stats = TcpConnectionTaskRemoteStatsWrapper::new(task_stats);
        wrapper_stats.push_other_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);
        let (ups_r, ups_w) = stream;
        (
            Box::new(LimitedReader::new(ups_r, wrapper_stats.clone())),
            Box::new(LimitedWriter::new(ups_w, wrapper_stats)),
        )
    }

    /// Wrap a stream from [`Self::tls_connect`] as one HTTP/1 origin request.
    ///
    /// Use after ALPN selects HTTP/1.1 (or no h2). Stats match
    /// `https_forward_new_connection`: task + user on TLS plaintext, no
    /// escaper IO on this layer. Then [`Self::prepare_reused_http_forward_connection`]
    /// so the first request is counted the same as a pooled H1 checkout.
    fn http_forward_from_tls_connection(
        &self,
        stream: TcpConnection,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> BoxHttpForwardConnection {
        let mut wrapper_stats = HttpForwardTaskRemoteWrapperStats::new(Arc::clone(&task_stats));
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);
        let (ups_r, ups_w) = stream;
        self.prepare_reused_http_forward_connection(
            (
                Box::new(TlsHttpForwardWriter::new(ups_w, wrapper_stats.clone())),
                Box::new(TlsHttpForwardReader::new(ups_r, wrapper_stats)),
            ),
            task_notes,
            task_stats,
            true,
        )
    }

    async fn publish(&self, data: &str) -> anyhow::Result<()>;

    async fn tcp_setup_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult;

    /// TLS connect when the application protocol is already fixed.
    ///
    /// Handshake, then attach task + user stats on the TLS plaintext. Use this
    /// when ALPN is a single known protocol (`tls_stream`, HTTPS forward, and
    /// other tasks that will not branch after the handshake). Escaper IO stats
    /// stay on the TCP layer under TLS.
    async fn tls_setup_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult;

    /// TLS connect when the application protocol is chosen after ALPN.
    ///
    /// Handshake only: escaper IO on TCP, no task/user wrap yet. Selected ALPN
    /// is stored on [`EgressNotes::selected_alpn`]. Returns the raw stream and
    /// the leaf that connected (`None` from a leaf — the caller already holds
    /// it). A route fills `Some(next)` where it selects the next hop, if that
    /// hop returned `None`. Upgrade on that escaper with
    /// [`Self::tls_connection_with_task_stats`] (h2) or
    /// [`Self::http_forward_from_tls_connection`] (http/1.1).
    async fn tls_connect(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        audit_ctx: &mut AuditContext,
    ) -> TlsConnectResult;

    async fn udp_setup_connection(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult;

    async fn udp_setup_relay(
        &self,
        task_conf: &UdpRelayTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> UdpRelaySetupResult;

    fn new_http_forward_context(&self, escaper: ArcEscaper) -> BoxHttpForwardContext;

    async fn new_ftp_connect_context(
        &self,
        escaper: ArcEscaper,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
    ) -> BoxFtpConnectContext;
}

pub(crate) type ArcEscaper = Arc<dyn Escaper + Send + Sync>;

/// Raw TLS connect: stream plus the leaf that connected, when a route selected one.
pub(crate) type TlsConnectResult = Result<(TcpConnection, Option<ArcEscaper>), TcpConnectError>;

pub(crate) trait EscaperExt: Escaper {
    fn select_consistent<'a, 'b, T>(
        &'a self,
        nodes: &'b SelectiveVec<T>,
        pick_policy: SelectivePickPolicy,
        task_notes: &'a ServerTaskNotes,
        host: &'a Host,
    ) -> &'b T
    where
        T: SelectiveItem,
    {
        #[derive(Hash)]
        struct ConsistentKey<'a> {
            client_ip: IpAddr,
            user: Option<&'a str>,
            host: &'a Host,
        }

        match pick_policy {
            SelectivePickPolicy::Random => nodes.pick_random(),
            SelectivePickPolicy::Serial => nodes.pick_serial(),
            SelectivePickPolicy::RoundRobin => nodes.pick_round_robin(),
            SelectivePickPolicy::Ketama => {
                let key = ConsistentKey {
                    client_ip: task_notes.client_ip(),
                    user: task_notes.raw_user_name().map(|s| s.as_ref()),
                    host,
                };
                nodes.pick_ketama(&key)
            }
            SelectivePickPolicy::Rendezvous => {
                let key = ConsistentKey {
                    client_ip: task_notes.client_ip(),
                    user: task_notes.raw_user_name().map(|s| s.as_ref()),
                    host,
                };
                nodes.pick_rendezvous(&key)
            }
            SelectivePickPolicy::JumpHash => {
                let key = ConsistentKey {
                    client_ip: task_notes.client_ip(),
                    user: task_notes.raw_user_name().map(|s| s.as_ref()),
                    host,
                };
                nodes.pick_jump(&key)
            }
        }
    }
}
