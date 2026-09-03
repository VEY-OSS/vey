/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use arcstr::ArcStr;
use jiff::Timestamp;
use tokio::time::Instant;
use uuid::Uuid;

use vey_daemon::server::ClientConnectionInfo;
use vey_types::limit::GaugeSemaphorePermit;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::resolve::ResolveRedirection;

use crate::auth::{UserContext, UserRequestStats, UserTrafficStats, UserUpstreamTrafficStats};
use crate::config::escaper::EgressUpstream;
use crate::escape::EgressPathSelection;
use crate::site::SiteContext;

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

/// server task notes is bounded to a single client connection.
/// it can be reset if the connection is consisted of many tasks.
/// Do not share this struct between different client connections.
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
    /// the following fields should not be cloned
    pub(crate) user_req_alive_permit: Option<GaugeSemaphorePermit>,
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
            user_req_alive_permit: None,
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

    pub(crate) fn resolve_redirection(&self) -> Option<&ResolveRedirection> {
        let user_ctx = if let Some(site_ctx) = &self.site_ctx {
            site_ctx.tenant()
        } else {
            self.user_ctx.as_ref()
        };
        user_ctx.and_then(|c| c.user().resolve_redirection())
    }

    pub(crate) fn foreach_req_stats<F>(&self, update: F)
    where
        F: Fn(&Arc<UserRequestStats>),
    {
        if let Some(site_ctx) = &self.site_ctx {
            update(site_ctx.origin_req_stats());
        }
        if let Some(user_ctx) = &self.user_ctx {
            user_ctx.foreach_req_stats(&update);
        }
    }

    pub(crate) fn fetch_traffic_stats(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Vec<Arc<UserTrafficStats>> {
        let mut all_stats = Vec::with_capacity(4);
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
    ) -> Vec<Arc<UserUpstreamTrafficStats>> {
        let mut all_stats = Vec::with_capacity(4);
        if let Some(site_ctx) = &self.site_ctx {
            all_stats.push(site_ctx.fetch_upstream_traffic_stats(escaper, escaper_extra_tags));
        }
        if let Some(user_ctx) = &self.user_ctx {
            all_stats.extend(user_ctx.fetch_upstream_traffic_stats(escaper, escaper_extra_tags));
        }
        all_stats
    }

    /// Idle ticks allowed for this task.
    /// Layers shrink with `min`: TenantUser → OriginSite → User.
    /// Missing layers are skipped; none set falls back to `server_default`.
    pub(crate) fn task_max_idle_count(&self, server_default: usize) -> usize {
        layered_task_idle_count(
            self.site_ctx
                .as_ref()
                .and_then(|s| s.tenant().and_then(|t| t.user().task_max_idle_count())),
            self.site_ctx
                .as_ref()
                .and_then(|s| s.origin().task_idle_max_count()),
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

    pub(crate) fn raw_user_name(&self) -> Option<&ArcStr> {
        self.user_ctx.as_ref().and_then(|c| c.raw_user_name())
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
