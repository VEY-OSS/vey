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
use vey_types::limit::{GaugeSemaphore, GaugeSemaphorePermit, GlobalRateLimitState, RateLimiter};
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{
    Host, HttpForwardedHeaderType, HttpKeepAliveConfig, OpensslClientConfig,
    OpensslServerConfigBuilder, TcpSockSpeedLimitConfig, UpstreamAddr,
};

use super::upstream::{SiteUpstream, UpstreamPeerStatus};
use super::{SiteHttp1Pool, SiteHttp2Pool, SiteStats};
use crate::auth::{UserForbiddenStats, UserGroup, UserRequestStats};
use crate::config::site::SiteConfig;

pub(crate) struct Site {
    config: Arc<SiteConfig>,
    tls_client: Option<OpensslClientConfig>,
    stats: Arc<SiteStats>,
    tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
    request_rate_limit: Option<Arc<RateLimiter<Arc<GlobalRateLimitState>>>>,
    req_alive_sem: Option<GaugeSemaphore>,
    h1_pool: Option<Arc<SiteHttp1Pool>>,
    h2_pool: Arc<SiteHttp2Pool>,
    upstream: SiteUpstream,
    forwarded_trusted_from: IpNetworkTable<()>,
}

impl Site {
    fn new_minimal(
        config: Arc<SiteConfig>,
        stats: Arc<SiteStats>,
        tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
        h1_pool: Option<Arc<SiteHttp1Pool>>,
        h2_pool: Arc<SiteHttp2Pool>,
        upstream: SiteUpstream,
    ) -> anyhow::Result<Self> {
        let tls_client = match &config.tls_client_builder {
            Some(builder) => Some(builder.build().context("failed to build TLS client")?),
            None => None,
        };
        let forwarded_trusted_from = config.http.build_forwarded_trusted_from_table();

        Ok(Site {
            config,
            tls_client,
            stats,
            tenant_user_group,
            request_rate_limit: None,
            req_alive_sem: None,
            h1_pool,
            h2_pool,
            upstream,
            forwarded_trusted_from,
        })
    }

    pub(super) fn new(
        site_group: &NodeName,
        config: Arc<SiteConfig>,
        tenant_user_group_name: &NodeName,
        tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
    ) -> anyhow::Result<Self> {
        let stats = Arc::new(SiteStats::new(
            site_group.clone(),
            config.id().clone(),
            config.owner().clone(),
            tenant_user_group_name.clone(),
        ));

        let is_tls = config.tls_client_builder.is_some();
        let h1_pool = config
            .http
            .h1
            .connection_pool
            .map(|cfg| Arc::new(SiteHttp1Pool::new(cfg, config.upstream(), is_tls)));
        let h2_pool = Arc::new(SiteHttp2Pool::new(
            config.http.h2.connection_pool,
            config.upstream(),
            is_tls,
        ));
        let upstream = SiteUpstream::new(config.upstream());

        let mut new =
            Self::new_minimal(config, stats, tenant_user_group, h1_pool, h2_pool, upstream)?;

        if let Some(cfg) = new.config.request_rate_limit {
            let limiter = RateLimiter::new_global_reloadable(cfg);
            new.request_rate_limit = Some(Arc::new(limiter));
        }
        if let Some(max_alive) = new.config.request_alive_max {
            new.req_alive_sem = Some(GaugeSemaphore::new(max_alive));
        }

        Ok(new)
    }

    pub(super) fn reload(
        &self,
        config: Arc<SiteConfig>,
        tenant_user_group_name: &NodeName,
        tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
    ) -> anyhow::Result<Self> {
        let is_tls = config.tls_client_builder.is_some();
        let h1_pool = config
            .http
            .h1
            .connection_pool
            .map(|cfg| match &self.h1_pool {
                Some(pool) => pool.new_or_reload(cfg, config.upstream(), is_tls),
                None => Arc::new(SiteHttp1Pool::new(cfg, config.upstream(), is_tls)),
            });
        let h2_pool =
            self.h2_pool
                .new_or_reload(config.http.h2.connection_pool, config.upstream(), is_tls);
        let upstream = self.upstream.reload(&config.upstream());

        let stats = if self
            .stats
            .same_tenant(config.owner(), tenant_user_group_name)
        {
            self.stats.clone()
        } else {
            Arc::new(SiteStats::new(
                self.site_group().clone(),
                config.id().clone(),
                config.owner().clone(),
                tenant_user_group_name.clone(),
            ))
        };

        let mut new =
            Self::new_minimal(config, stats, tenant_user_group, h1_pool, h2_pool, upstream)?;

        if let Some(cfg) = new.config.request_rate_limit {
            let limiter = match &self.request_rate_limit {
                Some(old) => old.reload(cfg),
                None => RateLimiter::new_global_reloadable(cfg),
            };
            new.request_rate_limit = Some(Arc::new(limiter));
        }
        if let Some(max_alive) = new.config.request_alive_max {
            let sema = match &self.req_alive_sem {
                Some(old) => old.new_updated(max_alive),
                None => GaugeSemaphore::new(max_alive),
            };
            new.req_alive_sem = Some(sema);
        }

        Ok(new)
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

    pub(crate) fn tls_inner_protocol(&self) -> Option<MaybeProtocol> {
        self.config.tls_inner_protocol
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
        self.h1_pool.as_deref()
    }

    pub(crate) fn http2_pool(&self) -> &SiteHttp2Pool {
        &self.h2_pool
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
