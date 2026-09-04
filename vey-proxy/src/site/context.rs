/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use arcstr::ArcStr;

use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{TcpConnectConfig, TcpKeepAliveConfig, TcpMiscSockOpts, UdpMiscSockOpts};
use vey_types::resolve::ResolveStrategy;

use super::{Site, SiteEgress};
use crate::auth::{
    UserContext, UserGroup, UserRequestStats, UserTrafficStats, UserUpstreamTrafficStats,
};
use crate::escape::EgressPathSelection;

/// Reverse-proxy task identity: required origin site, optional tenant user.
#[derive(Clone)]
pub(crate) struct SiteContext {
    origin: Arc<Site>,
    tenant: Option<UserContext>,
    origin_req_stats: Arc<UserRequestStats>,
    egress: Arc<SiteEgress>,
}

impl SiteContext {
    pub(crate) fn new(
        origin: Arc<Site>,
        egress: Arc<SiteEgress>,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Self {
        let tenant_group = origin.tenant_user_group();
        let tenant = lookup_tenant(
            origin.owner(),
            tenant_group.as_deref(),
            server,
            server_extra_tags,
        );
        let egress = match tenant.as_ref() {
            Some(t) => Arc::new(egress.shrink_with_tenant(Some(t))),
            None => egress,
        };
        let origin_req_stats = origin
            .stats()
            .fetch_request_stats(server, server_extra_tags);
        SiteContext {
            origin,
            tenant,
            origin_req_stats,
            egress,
        }
    }

    #[inline]
    pub(crate) fn origin(&self) -> &Arc<Site> {
        &self.origin
    }

    #[inline]
    pub(crate) fn tenant(&self) -> Option<&UserContext> {
        self.tenant.as_ref()
    }

    #[inline]
    pub(crate) fn origin_req_stats(&self) -> &Arc<UserRequestStats> {
        &self.origin_req_stats
    }

    pub(crate) fn rsp_hdr_recv_timeout(&self) -> Option<Duration> {
        self.origin.rsp_hdr_recv_timeout().or_else(|| {
            self.tenant
                .as_ref()
                .and_then(|t| t.http_rsp_header_recv_timeout())
        })
    }

    pub(crate) fn resolve_strategy(&self) -> Option<ResolveStrategy> {
        self.egress
            .resolve_strategy()
            .or_else(|| self.tenant.as_ref().and_then(|t| t.resolve_strategy()))
    }

    pub(crate) fn path_selection(&self) -> Option<&EgressPathSelection> {
        self.egress.path_selection().or_else(|| {
            self.tenant
                .as_ref()
                .and_then(|t| t.user_config().egress_path_selection.as_ref())
        })
    }

    #[inline]
    pub(crate) fn tcp_connect(&self) -> Option<&TcpConnectConfig> {
        self.egress.tcp_connect()
    }

    #[inline]
    pub(crate) fn tcp_remote_keepalive(&self) -> TcpKeepAliveConfig {
        self.egress.tcp_remote_keepalive()
    }

    #[inline]
    pub(crate) fn tcp_remote_misc_opts<'a>(
        &self,
        base_opts: &'a TcpMiscSockOpts,
    ) -> Cow<'a, TcpMiscSockOpts> {
        self.egress.tcp_remote_misc_opts(base_opts)
    }

    #[inline]
    pub(crate) fn udp_remote_misc_opts(&self, base_opts: &UdpMiscSockOpts) -> UdpMiscSockOpts {
        self.egress.udp_remote_misc_opts(base_opts)
    }

    pub(crate) fn fetch_traffic_stats(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserTrafficStats> {
        self.origin
            .stats()
            .fetch_traffic_stats(server, server_extra_tags)
    }

    pub(crate) fn fetch_upstream_traffic_stats(
        &self,
        escaper: &NodeName,
        escaper_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserUpstreamTrafficStats> {
        self.origin
            .stats()
            .fetch_upstream_traffic_stats(escaper, escaper_extra_tags)
    }

    pub(crate) fn check_rate_limit(&self) -> Result<(), ()> {
        if let Some(tenant) = &self.tenant {
            tenant.check_rate_limit()?;
        }
        self.origin.check_rate_limit()
    }
}

fn lookup_tenant(
    owner: &NodeName,
    tenant_group: Option<&UserGroup>,
    server: &NodeName,
    server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
) -> Option<UserContext> {
    if owner.is_empty() {
        return None;
    }
    let (user, user_type) = tenant_group?.get_named_user(owner.as_str())?;
    Some(UserContext::new(
        Some(ArcStr::from(owner.as_str())),
        user,
        user_type,
        server,
        server_extra_tags,
    ))
}
