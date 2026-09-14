/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;

use vey_types::limit::GaugeSemaphorePermit;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{TcpConnectConfig, TcpKeepAliveConfig, TcpMiscSockOpts, UdpMiscSockOpts};
use vey_types::resolve::ResolveStrategy;

use super::{Site, SiteEgress};
use crate::auth::{
    TenantContext, User, UserForbiddenStats, UserRequestStats, UserTrafficStats,
    UserUpstreamTrafficStats,
};
use crate::escape::EgressPathSelection;

/// Reverse-proxy task identity: required site, optional tenant user.
#[derive(Clone)]
pub(crate) struct SiteContext {
    site: Arc<Site>,
    tenant: Option<TenantContext>,
    req_stats: Arc<UserRequestStats>,
    forbid_stats: Arc<UserForbiddenStats>,
    egress: Arc<SiteEgress>,
}

impl SiteContext {
    pub(crate) fn new(
        site: Arc<Site>,
        egress: Arc<SiteEgress>,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Self {
        let req_stats = site.stats().fetch_request_stats(server, server_extra_tags);
        let forbid_stats = site
            .stats()
            .fetch_forbidden_stats(server, server_extra_tags);

        if let Some(group) = site.tenant_user_group()
            && let Some(tenant) = group.lookup_tenant(site.owner(), server, server_extra_tags)
        {
            let egress = egress.shrink_with_tenant(Some(tenant.user().as_ref()));
            SiteContext {
                site,
                tenant: Some(tenant),
                req_stats,
                forbid_stats,
                egress: Arc::new(egress),
            }
        } else {
            SiteContext {
                site,
                tenant: None,
                req_stats,
                forbid_stats,
                egress,
            }
        }
    }

    #[inline]
    pub(crate) fn site(&self) -> &Arc<Site> {
        &self.site
    }

    #[inline]
    pub(crate) fn tenant_ctx(&self) -> Option<&TenantContext> {
        self.tenant.as_ref()
    }

    #[inline]
    pub(crate) fn tenant_user(&self) -> Option<&Arc<User>> {
        self.tenant_ctx().map(|t| t.user())
    }

    #[inline]
    pub(crate) fn req_stats(&self) -> &Arc<UserRequestStats> {
        &self.req_stats
    }

    pub(crate) fn log_uri_max_chars(&self) -> Option<usize> {
        self.tenant_user().and_then(|u| u.log_uri_max_chars())
    }

    pub(crate) fn rsp_hdr_recv_timeout(&self) -> Option<Duration> {
        self.site.rsp_hdr_recv_timeout().or_else(|| {
            self.tenant_user()
                .and_then(|u| u.http_rsp_hdr_recv_timeout())
        })
    }

    pub(crate) fn resolve_strategy(&self) -> Option<ResolveStrategy> {
        self.egress
            .resolve_strategy()
            .or_else(|| self.tenant_user().and_then(|u| u.config().resolve_strategy))
    }

    pub(crate) fn path_selection(&self) -> Option<&EgressPathSelection> {
        self.egress.path_selection().or_else(|| {
            self.tenant_user()
                .and_then(|u| u.config().egress_path_selection.as_ref())
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
        self.site
            .stats()
            .fetch_traffic_stats(server, server_extra_tags)
    }

    pub(crate) fn fetch_upstream_traffic_stats(
        &self,
        escaper: &NodeName,
        escaper_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserUpstreamTrafficStats> {
        self.site
            .stats()
            .fetch_upstream_traffic_stats(escaper, escaper_extra_tags)
    }

    pub(crate) fn check_rate_limit(&self) -> Result<(), ()> {
        if let Some(tenant) = &self.tenant {
            tenant.check_rate_limit()?;
        }
        self.site.check_rate_limit(&self.forbid_stats)
    }

    /// Tenant first, then site. Each failure is counted on that principal.
    pub(crate) fn acquire_request_semaphores(&self) -> Result<SiteRequestPermits, ()> {
        let tenant = match &self.tenant {
            Some(t) => Some(t.acquire_request_semaphore()?),
            None => None,
        };
        let site = self.site.acquire_request_semaphore(&self.forbid_stats)?;
        Ok(SiteRequestPermits { tenant, site })
    }
}

/// Independent alive-request permits for the tenant user and the site.
#[derive(Default)]
pub(crate) struct SiteRequestPermits {
    tenant: Option<GaugeSemaphorePermit>,
    site: Option<GaugeSemaphorePermit>,
}

impl SiteRequestPermits {
    pub(crate) fn release(&mut self) {
        self.tenant.take();
        self.site.take();
    }
}
