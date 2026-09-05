/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use arcstr::ArcStr;
use foldhash::HashMap;

use vey_types::metrics::{MetricTagMap, NodeName};

use crate::auth::{
    UserForbiddenStats, UserRequestStats, UserTrafficStats, UserType, UserUpstreamTrafficStats,
};
use crate::stat::site::SiteMetricTags;

/// Reported as the tag value when a site metric identity field is unset.
const EMPTY_TAG: ArcStr = arcstr::literal!("-");

/// Per site counters behind the `site.*` metrics.
///
/// This mirrors [`UserSiteStats`](crate::auth::UserSiteStats) and reuses the same
/// counter types, but is keyed for the site alone: ingress counters are split
/// per server, egress counters per escaper. The user identity of those counters
/// is left empty; the site tags travel separately in [`SiteMetricTags`].
pub(crate) struct SiteStats {
    tags: SiteMetricTags,
    forbidden: Mutex<HashMap<NodeName, Arc<UserForbiddenStats>>>,
    request: Mutex<HashMap<NodeName, Arc<UserRequestStats>>>,
    client_io: Mutex<HashMap<NodeName, Arc<UserTrafficStats>>>,
    remote_io: Mutex<HashMap<NodeName, Arc<UserUpstreamTrafficStats>>>,
}

impl SiteStats {
    pub(super) fn new(
        site_group: &NodeName,
        site_id: &NodeName,
        owner: &NodeName,
        tenant_user_group: &NodeName,
    ) -> Self {
        SiteStats {
            tags: SiteMetricTags::new(
                site_group,
                site_id,
                tag_or_dash(owner),
                tag_or_dash(tenant_user_group),
            ),
            request: Mutex::new(HashMap::default()),
            forbidden: Mutex::new(HashMap::default()),
            client_io: Mutex::new(HashMap::default()),
            remote_io: Mutex::new(HashMap::default()),
        }
    }

    pub(crate) fn fetch_request_stats(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserRequestStats> {
        let mut new_stats = None;

        let mut map = self.request.lock().unwrap();
        let stats = map
            .entry(server.clone())
            .or_insert_with(|| {
                let stats = Arc::new(UserRequestStats::new(
                    &NodeName::default(),
                    ArcStr::default(),
                    UserType::Anonymous,
                    server,
                    server_extra_tags,
                ));
                new_stats = Some(stats.clone());
                stats
            })
            .clone();
        drop(map);

        if let Some(stats) = new_stats {
            crate::stat::site::push_request_stats(stats, &self.tags);
        }

        stats
    }

    pub(crate) fn fetch_forbidden_stats(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserForbiddenStats> {
        let mut new_stats = None;

        let mut map = self.forbidden.lock().unwrap();
        let stats = map
            .entry(server.clone())
            .or_insert_with(|| {
                let stats = Arc::new(UserForbiddenStats::new(
                    &NodeName::default(),
                    ArcStr::default(),
                    UserType::Anonymous,
                    server,
                    server_extra_tags,
                ));
                new_stats = Some(stats.clone());
                stats
            })
            .clone();
        drop(map);

        if let Some(stats) = new_stats {
            crate::stat::site::push_forbidden_stats(stats, &self.tags);
        }

        stats
    }

    pub(crate) fn fetch_traffic_stats(
        &self,
        server: &NodeName,
        server_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserTrafficStats> {
        let mut new_stats = None;

        let mut map = self.client_io.lock().unwrap();
        let stats = map
            .entry(server.clone())
            .or_insert_with(|| {
                let stats = Arc::new(UserTrafficStats::new(
                    &NodeName::default(),
                    ArcStr::default(),
                    UserType::Anonymous,
                    server,
                    server_extra_tags,
                ));
                new_stats = Some(stats.clone());
                stats
            })
            .clone();
        drop(map);

        if let Some(stats) = new_stats {
            crate::stat::site::push_traffic_stats(stats, &self.tags);
        }

        stats
    }

    pub(crate) fn fetch_upstream_traffic_stats(
        &self,
        escaper: &NodeName,
        escaper_extra_tags: &Arc<ArcSwapOption<MetricTagMap>>,
    ) -> Arc<UserUpstreamTrafficStats> {
        let mut new_stats = None;

        let mut map = self.remote_io.lock().unwrap();
        let stats = map
            .entry(escaper.clone())
            .or_insert_with(|| {
                let stats = Arc::new(UserUpstreamTrafficStats::new(
                    &NodeName::default(),
                    ArcStr::default(),
                    UserType::Anonymous,
                    escaper,
                    escaper_extra_tags,
                ));
                new_stats = Some(stats.clone());
                stats
            })
            .clone();
        drop(map);

        if let Some(stats) = new_stats {
            crate::stat::site::push_upstream_traffic_stats(stats, &self.tags);
        }

        stats
    }
}

fn tag_or_dash(name: &NodeName) -> ArcStr {
    if name.is_empty() {
        EMPTY_TAG
    } else {
        ArcStr::from(name.as_str())
    }
}
