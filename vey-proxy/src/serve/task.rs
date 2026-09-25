/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arc_swap::ArcSwapOption;
use arcstr::ArcStr;
use jiff::Timestamp;
use tokio::time::Instant;
use uuid::Uuid;

use vey_daemon::server::ClientConnectionInfo;
use vey_types::limit::GaugeSemaphorePermit;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::UpstreamAddr;
use vey_types::resolve::ResolveRedirection;

use crate::auth::{
    TenantContext, User, UserContext, UserRequestAliveGuard, UserRequestStats,
    UserRequestStatsList, UserTrafficStatsList, UserUpstreamTrafficStatsList,
};
use crate::config::escaper::EgressUpstream;
use crate::escape::EgressPathSelection;
use crate::site::{SiteContext, SiteRequestPermits};
use crate::stat::types::RequestAliveKind;

#[derive(Clone, Copy)]
pub(crate) enum ServerTaskStage {
    Created,
    Preparing,
    Connecting,
    Connected,
    Replying,
    LoggedIn,
    Relaying,
    Finished,
}

impl ServerTaskStage {
    pub(crate) fn brief(&self) -> &'static str {
        match self {
            ServerTaskStage::Created => "Created",
            ServerTaskStage::Preparing => "Preparing",
            ServerTaskStage::Connecting => "Connecting",
            ServerTaskStage::Connected => "Connected",
            ServerTaskStage::Replying => "Replying",
            ServerTaskStage::LoggedIn => "LoggedIn",
            ServerTaskStage::Relaying => "Relaying",
            ServerTaskStage::Finished => "Finished",
        }
    }
}

/// Per-task notes. `cc_info` is copied from the client connection; one
/// keep-alive connection may create many notes (one per request / stream).
/// Do not share a notes value across connections or tasks.
pub(crate) struct ServerTaskNotes {
    cc_info: ClientConnectionInfo,
    pub(crate) stage: ServerTaskStage,
    pub(crate) start_at: Timestamp,
    create_ins: Instant,
    pub(crate) id: Uuid,
    user_ctx: Option<UserContext>,
    site_ctx: Option<SiteContext>,
    pub(crate) wait_time: Duration,
    pub(crate) ready_time: Duration,
    pub(crate) egress_path_selection: Option<EgressPathSelection>,
    /// RAII: released when this notes is dropped
    _user_req_alive_permit: Option<GaugeSemaphorePermit>,
    _site_req_alive_permits: SiteRequestPermits,
    _req_alive_guard: Option<UserRequestAliveGuard>,
    origin: OnceLock<CachedUpstream>,
}

impl ServerTaskNotes {
    pub(crate) fn new(
        cc_info: ClientConnectionInfo,
        user_ctx: Option<UserContext>,
        wait_time: Duration,
    ) -> Self {
        ServerTaskNotes::with_path_selection(cc_info, user_ctx, wait_time, None)
    }

    pub(crate) fn with_path_selection(
        cc_info: ClientConnectionInfo,
        user_ctx: Option<UserContext>,
        wait_time: Duration,
        egress_path_selection: Option<EgressPathSelection>,
    ) -> Self {
        let started = Timestamp::now();
        let uuid = vey_daemon::server::task::generate_uuid(&started);
        ServerTaskNotes {
            cc_info,
            stage: ServerTaskStage::Created,
            start_at: started,
            create_ins: Instant::now(),
            id: uuid,
            user_ctx,
            site_ctx: None,
            wait_time,
            ready_time: Duration::default(),
            egress_path_selection,
            _user_req_alive_permit: None,
            _site_req_alive_permits: SiteRequestPermits::default(),
            _req_alive_guard: None,
            origin: OnceLock::new(),
        }
    }

    pub(crate) fn with_site_ctx(mut self, site_ctx: SiteContext) -> Self {
        self.site_ctx = Some(site_ctx);
        self
    }

    #[inline]
    pub(crate) fn client_addr(&self) -> SocketAddr {
        self.cc_info.client_addr()
    }

    #[inline]
    pub(crate) fn client_ip(&self) -> IpAddr {
        self.cc_info.client_ip()
    }

    /// Upstream chosen for this task. The first call selects; later calls reuse it.
    pub(crate) fn site_upstream(&self) -> anyhow::Result<&UpstreamAddr> {
        let cached = self.cached_upstream();
        if let Some(error) = &cached.error {
            Err(anyhow::anyhow!("{error}"))
        } else {
            Ok(&cached.addr)
        }
    }

    pub(crate) fn site_upstream_addr(&self) -> &UpstreamAddr {
        &self.cached_upstream().addr
    }

    pub(crate) fn site_upstream_peer(&self) -> Option<SocketAddr> {
        self.site_upstream()
            .ok()
            .and_then(UpstreamAddr::socket_addr)
    }

    fn cached_upstream(&self) -> &CachedUpstream {
        let ip = self.client_ip();
        let site = self.site_ctx.as_ref().map(|ctx| Arc::clone(ctx.site()));
        self.origin.get_or_init(|| match site {
            Some(site) => match site.select_upstream(ip) {
                Ok(addr) => CachedUpstream { addr, error: None },
                Err(e) => CachedUpstream {
                    addr: UpstreamAddr::empty(),
                    error: Some(e.to_string()),
                },
            },
            None => CachedUpstream {
                addr: UpstreamAddr::empty(),
                error: Some("no site context".to_string()),
            },
        })
    }

    #[inline]
    pub(crate) fn server_addr(&self) -> SocketAddr {
        self.cc_info.server_addr()
    }

    #[inline]
    pub(crate) fn worker_id(&self) -> Option<usize> {
        self.cc_info.worker_id()
    }

    #[inline]
    pub(crate) fn user_ctx(&self) -> Option<&UserContext> {
        self.user_ctx.as_ref()
    }

    #[inline]
    pub(crate) fn user_ctx_mut(&mut self) -> Option<&mut UserContext> {
        self.user_ctx.as_mut()
    }

    #[inline]
    pub(crate) fn site_ctx(&self) -> Option<&SiteContext> {
        self.site_ctx.as_ref()
    }

    #[inline]
    pub(crate) fn tenant_ctx(&self) -> Option<&TenantContext> {
        self.site_ctx.as_ref().and_then(|s| s.tenant_ctx())
    }

    #[inline]
    pub(crate) fn tenant_user(&self) -> Option<&Arc<User>> {
        self.tenant_ctx().map(|t| t.user())
    }

    pub(crate) fn resolve_redirection(&self) -> Option<&ResolveRedirection> {
        if let Some(site_ctx) = &self.site_ctx {
            site_ctx.tenant_user().and_then(|u| u.resolve_redirection())
        } else {
            self.user_ctx
                .as_ref()
                .and_then(|c| c.user().resolve_redirection())
        }
    }

    pub(crate) fn foreach_req_stats<F>(&self, mut update: F)
    where
        F: FnMut(&Arc<UserRequestStats>),
    {
        if let Some(site_ctx) = &self.site_ctx {
            update(site_ctx.req_stats());
        }
        if let Some(user_ctx) = &self.user_ctx {
            user_ctx.foreach_req_stats(update);
        }
    }

    /// Count `req_total` now and hold `req_alive` until this notes value is dropped.
    pub(crate) fn hold_req_alive(&mut self, kind: RequestAliveKind) {
        let mut stats = UserRequestStatsList::new();
        self.foreach_req_stats(|s| {
            kind.add_total(&s.req_total);
            stats.push(Arc::clone(s));
        });
        if !stats.is_empty() {
            self._req_alive_guard = Some(UserRequestAliveGuard::hold(kind, stats));
        }
    }

    /// Add another stats layer to the alive guard (SOCKS UDP site after first packet).
    pub(crate) fn extend_req_alive(&mut self, extra: Arc<UserRequestStats>) {
        if let Some(guard) = &mut self._req_alive_guard {
            guard.add(extra);
        }
    }

    pub(crate) fn fetch_traffic_stats(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> UserTrafficStatsList {
        let mut all_stats = UserTrafficStatsList::new();
        if let Some(site_ctx) = &self.site_ctx {
            all_stats.push(site_ctx.fetch_traffic_stats(server, server_extra_tags));
        }
        if let Some(user_ctx) = &self.user_ctx {
            all_stats.extend(user_ctx.fetch_traffic_stats(server, server_extra_tags));
        }
        all_stats
    }

    pub(crate) fn fetch_upstream_traffic_stats(
        &self,
        escaper: &NodeName,
        escaper_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> UserUpstreamTrafficStatsList {
        let mut all_stats = UserUpstreamTrafficStatsList::new();
        if let Some(site_ctx) = &self.site_ctx {
            all_stats.push(site_ctx.fetch_upstream_traffic_stats(escaper, escaper_extra_tags));
        }
        if let Some(user_ctx) = &self.user_ctx {
            all_stats.extend(user_ctx.fetch_upstream_traffic_stats(escaper, escaper_extra_tags));
        }
        all_stats
    }

    /// Idle ticks allowed for this task.
    /// Layers shrink with `min`: TenantContext → Site → User.
    /// Missing layers are skipped; none set falls back to `server_default`.
    pub(crate) fn task_max_idle_count(&self, server_default: usize) -> usize {
        layered_task_idle_count(
            self.tenant_user().and_then(|u| u.task_max_idle_count()),
            self.site_ctx
                .as_ref()
                .and_then(|s| s.site().task_idle_max_count()),
            self.user_ctx
                .as_ref()
                .and_then(|c| c.user().task_max_idle_count()),
            server_default,
        )
    }

    pub(crate) fn check_layered_rate_limit(&self) -> Result<(), ()> {
        if let Some(site_ctx) = &self.site_ctx {
            site_ctx.check_rate_limit()?;
        }
        if let Some(user_ctx) = &self.user_ctx {
            user_ctx.check_rate_limit()?;
        }
        Ok(())
    }

    /// Tenant then site. No-op when this notes has no site.
    pub(crate) fn acquire_site_request_semaphores(&mut self) -> Result<(), ()> {
        let Some(site_ctx) = &self.site_ctx else {
            return Ok(());
        };
        self._site_req_alive_permits = site_ctx.acquire_request_semaphores()?;
        Ok(())
    }

    /// No-op when this notes has no user.
    pub(crate) fn acquire_user_request_semaphore(&mut self) -> Result<(), ()> {
        let Some(user_ctx) = &self.user_ctx else {
            return Ok(());
        };
        self._user_req_alive_permit = Some(user_ctx.acquire_request_semaphore()?);
        Ok(())
    }

    pub(crate) fn raw_user_name(&self) -> Option<&ArcStr> {
        self.user_ctx.as_ref().and_then(|c| c.raw_user_name())
    }

    pub(crate) fn tenant_user_name(&self) -> Option<&ArcStr> {
        self.tenant_user().map(|u| u.name())
    }

    pub(crate) fn site_id(&self) -> Option<&NodeName> {
        self.site_ctx.as_ref().map(|s| s.site().id())
    }

    pub(crate) fn egress_path_number_id(&self, escaper: &NodeName, length: usize) -> Option<usize> {
        if let Some(site_ctx) = &self.site_ctx {
            if let Some(p) = site_ctx.path_selection()
                && let Some(id) = p.select_number_id(escaper, length)
            {
                return Some(id);
            }
        } else if let Some(ctx) = &self.user_ctx
            && let Some(p) = ctx.user_config().egress_path_selection.as_ref()
            && let Some(id) = p.select_number_id(escaper, length)
        {
            return Some(id);
        }

        if let Some(p) = &self.egress_path_selection {
            p.select_number_id(escaper, length)
        } else {
            None
        }
    }

    pub(crate) fn egress_path_string_id(&self, escaper: &NodeName) -> Option<ArcStr> {
        if let Some(site_ctx) = &self.site_ctx {
            if let Some(p) = site_ctx.path_selection()
                && let Some(id) = p.select_string_id(escaper)
            {
                return Some(id);
            }
        } else if let Some(ctx) = &self.user_ctx
            && let Some(p) = ctx.user_config().egress_path_selection.as_ref()
            && let Some(id) = p.select_string_id(escaper)
        {
            return Some(id);
        }

        if let Some(p) = &self.egress_path_selection {
            p.select_string_id(escaper)
        } else {
            None
        }
    }

    pub(crate) fn egress_path_upstream(&self, escaper: &NodeName) -> Option<Arc<EgressUpstream>> {
        if let Some(site_ctx) = &self.site_ctx {
            if let Some(p) = site_ctx.path_selection()
                && let Some(addr) = p.select_upstream(escaper)
            {
                return Some(addr);
            }
        } else if let Some(ctx) = &self.user_ctx
            && let Some(p) = ctx.user_config().egress_path_selection.as_ref()
            && let Some(addr) = p.select_upstream(escaper)
        {
            return Some(addr);
        }

        if let Some(p) = &self.egress_path_selection {
            p.select_upstream(escaper)
        } else {
            None
        }
    }

    pub(crate) fn egress_path_json_value(&self, escaper: &NodeName) -> Option<&serde_json::Value> {
        if let Some(site_ctx) = &self.site_ctx {
            if let Some(p) = site_ctx.path_selection()
                && let Some(value) = p.select_json_value(escaper)
            {
                return Some(value);
            }
        } else if let Some(ctx) = &self.user_ctx
            && let Some(p) = ctx.user_config().egress_path_selection.as_ref()
            && let Some(value) = p.select_json_value(escaper)
        {
            return Some(value);
        }

        if let Some(p) = &self.egress_path_selection {
            p.select_json_value(escaper)
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn task_created_instant(&self) -> Instant {
        self.create_ins
    }

    #[inline]
    pub(crate) fn time_elapsed(&self) -> Duration {
        self.create_ins.elapsed()
    }

    pub(crate) fn mark_relaying(&mut self) {
        self.stage = ServerTaskStage::Relaying;
        self.ready_time = self.create_ins.elapsed();
        if let Some(user_ctx) = &self.user_ctx {
            user_ctx.record_task_ready(self.ready_time);
        }
    }
}

struct CachedUpstream {
    addr: UpstreamAddr,
    error: Option<String>,
}

fn layered_task_idle_count(
    tenant: Option<usize>,
    origin: Option<usize>,
    user: Option<usize>,
    server_default: usize,
) -> usize {
    [tenant, origin, user]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(server_default)
}

#[cfg(test)]
mod tests {
    use super::layered_task_idle_count;

    #[test]
    fn idle_count_shrinks_in_stack_order() {
        assert_eq!(layered_task_idle_count(None, None, None, 10), 10);
        assert_eq!(layered_task_idle_count(Some(8), None, None, 10), 8);
        assert_eq!(layered_task_idle_count(Some(8), Some(3), Some(6), 10), 3);
        assert_eq!(layered_task_idle_count(None, Some(5), Some(9), 10), 5);
        assert_eq!(layered_task_idle_count(None, None, Some(7), 10), 7);
    }
}
