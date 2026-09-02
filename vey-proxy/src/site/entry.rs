/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::Context;

use vey_types::limit::{
    GaugeSemaphore, GaugeSemaphorePermit, GlobalRateLimitState, RateLimitQuota, RateLimiter,
};
use vey_types::metrics::NodeName;
use vey_types::net::{
    Host, OpensslClientConfig, OpensslServerConfigBuilder, TcpSockSpeedLimitConfig, UpstreamAddr,
};

use super::SiteStats;
use crate::config::site::SiteConfig;

pub(crate) struct Site {
    config: Arc<SiteConfig>,
    tls_client: Option<OpensslClientConfig>,
    stats: Arc<SiteStats>,
    request_rate_limit: Option<Arc<RateLimiter<GlobalRateLimitState>>>,
    req_alive_sem: Option<GaugeSemaphore>,
}

impl Site {
    pub(super) fn try_build(
        site_group: &NodeName,
        config: &Arc<SiteConfig>,
    ) -> anyhow::Result<Self> {
        let tls_client = build_tls_client(config)?;
        let request_rate_limit = config
            .request_rate_limit
            .map(|quota| Arc::new(RateLimiter::new_global(quota)));
        let req_alive_sem = config.request_alive_max.map(GaugeSemaphore::new);

        Ok(Site {
            config: Arc::clone(config),
            tls_client,
            stats: Arc::new(SiteStats::new(site_group, config.id())),
            request_rate_limit,
            req_alive_sem,
        })
    }

    pub(super) fn new_for_reload(&self, config: &Arc<SiteConfig>) -> anyhow::Result<Self> {
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

        Ok(Site {
            config: Arc::clone(config),
            tls_client,
            stats: Arc::clone(&self.stats),
            request_rate_limit,
            req_alive_sem,
        })
    }

    pub(crate) fn id(&self) -> &NodeName {
        self.config.id()
    }

    pub(crate) fn upstream(&self) -> &UpstreamAddr {
        self.config.upstream()
    }

    pub(crate) fn tls_name(&self) -> &Host {
        &self.config.tls_name
    }

    pub(crate) fn tls_server_builder(&self) -> Option<&OpensslServerConfigBuilder> {
        self.config.tls_server_builder.as_ref()
    }

    pub(crate) fn tls_client(&self) -> Option<&OpensslClientConfig> {
        self.tls_client.as_ref()
    }

    pub(crate) fn stats(&self) -> &Arc<SiteStats> {
        &self.stats
    }

    pub(crate) fn tcp_sock_speed_limit(&self) -> &TcpSockSpeedLimitConfig {
        &self.config.tcp_sock_speed_limit
    }

    pub(crate) fn check_rate_limit(&self) -> Result<(), ()> {
        if let Some(limit) = &self.request_rate_limit
            && limit.check().is_err()
        {
            return Err(());
        }
        Ok(())
    }

    pub(crate) fn acquire_request_semaphore(&self) -> Result<Option<GaugeSemaphorePermit>, ()> {
        self.req_alive_sem
            .as_ref()
            .map(|sem| sem.try_acquire().map_err(|_| {}))
            .transpose()
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
