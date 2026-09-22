/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use arc_swap::ArcSwapOption;
use ip_network_table::IpNetworkTable;

use vey_dpi::MaybeProtocol;
use vey_types::limit::{
    GaugeSemaphore, GaugeSemaphorePermit, GlobalRateLimitState, RateLimitQuota, RateLimiter,
};
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{
    Host, HttpForwardedHeaderType, HttpKeepAliveConfig, OpensslClientConfig,
    OpensslServerConfigBuilder, TcpSockSpeedLimitConfig, UpstreamAddr,
};

use super::SiteStats;
use super::pool::{SiteHttp1Pool, SiteHttp2Pool};
use super::upstream::{SiteUpstream, UpstreamPeerStatus};
use crate::auth::{UserForbiddenStats, UserGroup, UserRequestStats};
use crate::config::site::SiteConfig;

pub(crate) struct Site {
    config: Arc<SiteConfig>,
    tls_client: Option<OpensslClientConfig>,
    stats: Arc<SiteStats>,
    tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
    request_rate_limit: Option<Arc<RateLimiter<GlobalRateLimitState>>>,
    req_alive_sem: Option<GaugeSemaphore>,
    http1_pool: Option<Arc<SiteHttp1Pool>>,
    http2_pool: Arc<SiteHttp2Pool>,
    upstream: SiteUpstream,
    forwarded_trusted_from: IpNetworkTable<()>,
}

impl Site {
    pub(super) fn try_build(
        site_group: &NodeName,
        config: &Arc<SiteConfig>,
        tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
    ) -> anyhow::Result<Self> {
        let tls_client = build_tls_client(config)?;
        let request_rate_limit = config
            .request_rate_limit
            .map(|quota| Arc::new(RateLimiter::new_global(quota)));
        let req_alive_sem = config.request_alive_max.map(GaugeSemaphore::new);
        let tenant_user_group_name = tenant_user_group
            .load()
            .as_ref()
            .map(|g| g.name().clone())
            .unwrap_or_default();

        Ok(Site {
            config: Arc::clone(config),
            tls_client,
            stats: Arc::new(SiteStats::new(
                site_group,
                config.id(),
                config.owner(),
                &tenant_user_group_name,
            )),
            tenant_user_group,
            request_rate_limit,
            req_alive_sem,
            http1_pool: config
                .http
                .h1
                .connection_pool
                .map(|cfg| Arc::new(SiteHttp1Pool::new(cfg))),
            http2_pool: Arc::new(SiteHttp2Pool::new(config.http.h2.connection_pool)),
            upstream: SiteUpstream::from_config(config.upstream()),
            forwarded_trusted_from: config.http.build_forwarded_trusted_from_table(),
        })
    }

    pub(super) fn new_for_reload(
        &self,
        config: &Arc<SiteConfig>,
        tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
    ) -> anyhow::Result<Self> {
        let tls_client = build_tls_client(config)?;
        let request_rate_limit = reuse_or_new_rate_limiter(
            &self.request_rate_limit,
            self.config.request_rate_limit,
            config.request_rate_limit,
        );
        let req_alive_sem = config.request_alive_max.map(|permits| {
            self.req_alive_sem
                .as_ref()
                .map(|sema| sema.new_updated(permits))
                .unwrap_or_else(|| GaugeSemaphore::new(permits))
        });
        let http1_pool = reuse_or_new_http1_pool(self, config);
        let http2_pool = reuse_or_new_http2_pool(self, config);

        Ok(Site {
            config: Arc::clone(config),
            tls_client,
            stats: Arc::clone(&self.stats),
            tenant_user_group,
            request_rate_limit,
            req_alive_sem,
            http1_pool,
            http2_pool,
            upstream: SiteUpstream::new_for_reload(&self.upstream, config.upstream()),
            forwarded_trusted_from: config.http.build_forwarded_trusted_from_table(),
        })
    }

    pub(crate) fn id(&self) -> &NodeName {
        self.config.id()
    }

    pub(crate) fn owner(&self) -> &NodeName {
        self.config.owner()
    }

    pub(super) fn refresh_tenant_user_group(
        &self,
        name: &NodeName,
        group: Option<Arc<UserGroup>>,
    ) -> bool {
        if self
            .tenant_user_group
            .load()
            .as_ref()
            .is_some_and(|current| current.name().eq(name))
        {
            self.tenant_user_group.store(group);
            true
        } else {
            false
        }
    }

    pub(crate) fn tenant_user_group(&self) -> Option<Arc<UserGroup>> {
        self.tenant_user_group.load_full()
    }

    pub(crate) fn select_upstream(&self, client_ip: IpAddr) -> anyhow::Result<UpstreamAddr> {
        self.upstream.select(client_ip)
    }

    pub(crate) fn list_upstream_peers(&self) -> anyhow::Result<Vec<UpstreamPeerStatus>> {
        self.upstream.list_peers()
    }

    pub(crate) fn set_upstream_weight(
        &self,
        addr: std::net::SocketAddr,
        weight: f64,
    ) -> anyhow::Result<()> {
        self.upstream.set_weight(addr, weight)
    }

    pub(crate) fn tls_name(&self) -> &Host {
        &self.config.tls_name
    }

    /// Configured SNI name, or the request host when `tls_name` is unset.
    pub(crate) fn tls_name_or<'a>(&'a self, request_host: &'a Host) -> &'a Host {
        let configured = self.tls_name();
        if configured.is_empty() {
            request_host
        } else {
            configured
        }
    }

    pub(crate) fn covers_host(&self, host: &Host) -> bool {
        self.config.covers_host(host)
    }

    pub(crate) fn dpi_protocol(&self) -> Option<MaybeProtocol> {
        self.config.dpi_protocol
    }

    pub(crate) fn tls_server_builder(&self) -> Option<&OpensslServerConfigBuilder> {
        self.config.tls_server_builder.as_ref()
    }

    pub(crate) fn tls_client(&self) -> Option<&OpensslClientConfig> {
        self.tls_client.as_ref()
    }

    #[inline]
    pub(crate) fn config(&self) -> &Arc<SiteConfig> {
        &self.config
    }

    pub(crate) fn stats(&self) -> &Arc<SiteStats> {
        &self.stats
    }

    pub(super) fn site_group(&self) -> &NodeName {
        self.stats.site_group()
    }

    pub(crate) fn tcp_sock_speed_limit(&self) -> TcpSockSpeedLimitConfig {
        self.config.tcp_sock_speed_limit
    }

    #[inline]
    pub(crate) fn task_idle_max_count(&self) -> Option<usize> {
        self.config.task_idle_max_count
    }

    pub(crate) fn rsp_hdr_recv_timeout(&self) -> Option<Duration> {
        self.config.http.rsp_hdr_recv_timeout
    }

    #[inline]
    pub(crate) fn h1_keepalive_config(&self) -> HttpKeepAliveConfig {
        self.config.http.h1.upstream_keepalive
    }

    pub(crate) fn http1_pool(&self) -> Option<&SiteHttp1Pool> {
        self.http1_pool.as_deref()
    }

    pub(crate) fn http2_pool(&self) -> &SiteHttp2Pool {
        &self.http2_pool
    }

    pub(crate) fn trusts_forwarded_from(&self, ip: IpAddr) -> bool {
        self.forwarded_trusted_from.longest_match(ip).is_some()
    }

    #[inline]
    pub(crate) fn forwarded_header_type(&self) -> HttpForwardedHeaderType {
        self.config.http.forwarded_header_type
    }

    pub(crate) fn check_rate_limit(&self, forbid: &UserForbiddenStats) -> Result<(), ()> {
        if let Some(limit) = &self.request_rate_limit
            && limit.check().is_err()
        {
            forbid.add_rate_limited();
            return Err(());
        }
        Ok(())
    }

    pub(crate) fn acquire_request_semaphore(
        &self,
        forbid: &UserForbiddenStats,
    ) -> Result<Option<GaugeSemaphorePermit>, ()> {
        self.req_alive_sem
            .as_ref()
            .map(|sem| {
                sem.try_acquire().map_err(|_| {
                    forbid.add_fully_loaded();
                })
            })
            .transpose()
    }

    /// Count one client HTTP connection against this site until the guard drops.
    pub(crate) fn hold_http_conn(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> SiteHttpConnGuard {
        let stats = self.stats.fetch_request_stats(server, server_extra_tags);
        stats.conn_total.add_http();
        stats.l7_conn_alive.inc_http();
        SiteHttpConnGuard { stats }
    }
}

/// Drops `l7_conn_alive` for the site HTTP connection counted by [`Site::hold_http_conn`].
pub(crate) struct SiteHttpConnGuard {
    stats: Arc<UserRequestStats>,
}

impl Drop for SiteHttpConnGuard {
    fn drop(&mut self) {
        self.stats.l7_conn_alive.dec_http();
    }
}

fn build_tls_client(config: &SiteConfig) -> anyhow::Result<Option<OpensslClientConfig>> {
    if let Some(builder) = &config.tls_client_builder {
        let client = builder.build().context("failed to build tls client")?;
        Ok(Some(client))
    } else {
        Ok(None)
    }
}

fn reuse_or_new_http1_pool(old: &Site, config: &SiteConfig) -> Option<Arc<SiteHttp1Pool>> {
    let pool_cfg = config.http.h1.connection_pool?;
    if old.http1_pool.is_some()
        && old.config.http.h1.connection_pool == Some(pool_cfg)
        && old.config.upstream() == config.upstream()
        && old.config.tls_client_builder == config.tls_client_builder
        && old.config.tls_name == config.tls_name
    {
        return old.http1_pool.clone();
    }
    Some(Arc::new(SiteHttp1Pool::new(pool_cfg)))
}

fn reuse_or_new_http2_pool(old: &Site, config: &SiteConfig) -> Arc<SiteHttp2Pool> {
    if old.config.http.h2.connection_pool == config.http.h2.connection_pool
        && old.config.upstream() == config.upstream()
        && old.config.tls_client_builder == config.tls_client_builder
        && old.config.tls_name == config.tls_name
    {
        return Arc::clone(&old.http2_pool);
    }
    Arc::new(SiteHttp2Pool::new(config.http.h2.connection_pool))
}

fn reuse_or_new_rate_limiter(
    old_limiter: &Option<Arc<RateLimiter<GlobalRateLimitState>>>,
    old_quota: Option<RateLimitQuota>,
    new_quota: Option<RateLimitQuota>,
) -> Option<Arc<RateLimiter<GlobalRateLimitState>>> {
    match new_quota {
        Some(quota) => {
            if let (Some(old_limiter), Some(old_quota)) = (old_limiter, old_quota)
                && quota.eq(&old_quota)
            {
                Some(Arc::clone(old_limiter))
            } else {
                Some(Arc::new(RateLimiter::new_global(quota)))
            }
        }
        None => None,
    }
}
