/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use arcstr::ArcStr;

use vey_daemon::metrics::{TAG_KEY_SERVER, TAG_KEY_STAT_ID};
use vey_statsd_client::{StatsdClient, StatsdTagGroup};
use vey_types::metrics::NodeName;
use vey_types::stats::{GlobalStatsMap, StatId};

use super::{RequestStatsNamesRef, TAG_KEY_ESCAPER, TrafficStatsNamesRef};
use crate::auth::{
    UserRequestSnapshot, UserRequestStats, UserTrafficSnapshot, UserTrafficStats,
    UserUpstreamTrafficSnapshot, UserUpstreamTrafficStats,
};

const TAG_KEY_SITE_GROUP: &str = "site_group";
const TAG_KEY_SITE: &str = "site";
const TAG_KEY_TENANT: &str = "tenant";

const REQUEST_STATS_NAMES: RequestStatsNamesRef<'static> = RequestStatsNamesRef {
    connection_total: "site.connection.total",
    request_total: "site.request.total",
    request_alive: "site.request.alive",
    request_ready: "site.request.ready",
    request_reuse: "site.request.reuse",
    request_renew: "site.request.renew",
    l7_connection_alive: "site.l7.connection.alive",
};

const CLIENT_TRAFFIC_STATS_NAMES: TrafficStatsNamesRef<'static> = TrafficStatsNamesRef {
    in_bytes: "site.traffic.in.bytes",
    in_packets: "site.traffic.in.packets",
    out_bytes: "site.traffic.out.bytes",
    out_packets: "site.traffic.out.packets",
};

const UPSTREAM_TRAFFIC_STATS_NAMES: TrafficStatsNamesRef<'static> = TrafficStatsNamesRef {
    in_bytes: "site.upstream.traffic.in.bytes",
    in_packets: "site.upstream.traffic.in.packets",
    out_bytes: "site.upstream.traffic.out.bytes",
    out_packets: "site.upstream.traffic.out.packets",
};

static STORE_REQUEST_STATS_MAP: Mutex<GlobalStatsMap<RequestStatsValue>> =
    Mutex::new(GlobalStatsMap::new());
static STORE_TRAFFIC_STATS_MAP: Mutex<GlobalStatsMap<TrafficStatsValue>> =
    Mutex::new(GlobalStatsMap::new());
static STORE_UPSTREAM_TRAFFIC_STATS_MAP: Mutex<GlobalStatsMap<UpstreamTrafficStatsValue>> =
    Mutex::new(GlobalStatsMap::new());

static SITE_REQUEST_STATS_MAP: Mutex<GlobalStatsMap<RequestStatsValue>> =
    Mutex::new(GlobalStatsMap::new());
static SITE_TRAFFIC_STATS_MAP: Mutex<GlobalStatsMap<TrafficStatsValue>> =
    Mutex::new(GlobalStatsMap::new());
static SITE_UPSTREAM_TRAFFIC_STATS_MAP: Mutex<GlobalStatsMap<UpstreamTrafficStatsValue>> =
    Mutex::new(GlobalStatsMap::new());

/// The site identity carried by every `site.*` metric.
///
/// The counter types are shared with `user.*`, but their `user_group` / `user`
/// fields are left empty for sites. The site tags are recorded here instead, so
/// nothing about the site is ever reported through a user tag.
#[derive(Clone)]
pub(crate) struct SiteMetricTags {
    group: NodeName,
    id: NodeName,
    /// Site owner, already normalized to `-` when the site has none.
    tenant: ArcStr,
}

impl SiteMetricTags {
    pub(crate) fn new(group: &NodeName, id: &NodeName, tenant: ArcStr) -> Self {
        SiteMetricTags {
            group: group.clone(),
            id: id.clone(),
            tenant,
        }
    }

    fn add_to(&self, tags: &mut StatsdTagGroup, stat_id: StatId) {
        let mut buffer = itoa::Buffer::new();
        let stat_id = buffer.format(stat_id.as_u64());
        tags.add_tag(TAG_KEY_SITE_GROUP, &self.group);
        tags.add_tag(TAG_KEY_SITE, &self.id);
        tags.add_tag(TAG_KEY_TENANT, &self.tenant);
        tags.add_tag(TAG_KEY_STAT_ID, stat_id);
    }
}

struct RequestStatsValue {
    stats: Arc<UserRequestStats>,
    snap: UserRequestSnapshot,
    site: SiteMetricTags,
}

struct TrafficStatsValue {
    stats: Arc<UserTrafficStats>,
    snap: UserTrafficSnapshot,
    site: SiteMetricTags,
}

struct UpstreamTrafficStatsValue {
    stats: Arc<UserUpstreamTrafficStats>,
    snap: UserUpstreamTrafficSnapshot,
    site: SiteMetricTags,
}

pub(crate) fn push_request_stats(stats: Arc<UserRequestStats>, site: &SiteMetricTags) {
    let k = stats.stat_id();
    let v = RequestStatsValue {
        stats,
        snap: Default::default(),
        site: site.clone(),
    };
    let mut ht = STORE_REQUEST_STATS_MAP.lock().unwrap();
    ht.insert(k, v);
}

pub(crate) fn push_traffic_stats(stats: Arc<UserTrafficStats>, site: &SiteMetricTags) {
    let k = stats.stat_id();
    let v = TrafficStatsValue {
        stats,
        snap: Default::default(),
        site: site.clone(),
    };
    let mut ht = STORE_TRAFFIC_STATS_MAP.lock().unwrap();
    ht.insert(k, v);
}

pub(crate) fn push_upstream_traffic_stats(
    stats: Arc<UserUpstreamTrafficStats>,
    site: &SiteMetricTags,
) {
    let k = stats.stat_id();
    let v = UpstreamTrafficStatsValue {
        stats,
        snap: Default::default(),
        site: site.clone(),
    };
    let mut ht = STORE_UPSTREAM_TRAFFIC_STATS_MAP.lock().unwrap();
    ht.insert(k, v);
}

pub(in crate::stat) fn sync_stats() {
    use vey_daemon::metrics::helper::move_ht;

    move_ht(&STORE_REQUEST_STATS_MAP, &SITE_REQUEST_STATS_MAP);
    move_ht(&STORE_TRAFFIC_STATS_MAP, &SITE_TRAFFIC_STATS_MAP);
    move_ht(
        &STORE_UPSTREAM_TRAFFIC_STATS_MAP,
        &SITE_UPSTREAM_TRAFFIC_STATS_MAP,
    );
}

pub(in crate::stat) fn emit_stats(client: &mut StatsdClient) {
    let mut req_stats_map = SITE_REQUEST_STATS_MAP.lock().unwrap();
    req_stats_map.retain(|v| {
        let mut common_tags = StatsdTagGroup::default();
        v.site.add_to(&mut common_tags, v.stats.stat_id());
        common_tags.add_tag(TAG_KEY_SERVER, v.stats.server());
        if let Some(server_extra_tags) = v.stats.server_extra_tags() {
            common_tags.add_static_tags(&server_extra_tags);
        }
        super::user::emit_request_stats_with_tags(
            client,
            &v.stats,
            &mut v.snap,
            &REQUEST_STATS_NAMES,
            &common_tags,
        );
        // use Arc instead of Weak here, as we should emit the final metrics before drop it
        Arc::strong_count(&v.stats) > 1
    });
    drop(req_stats_map);

    let mut io_stats_map = SITE_TRAFFIC_STATS_MAP.lock().unwrap();
    io_stats_map.retain(|v| {
        let mut common_tags = StatsdTagGroup::default();
        v.site.add_to(&mut common_tags, v.stats.stat_id());
        common_tags.add_tag(TAG_KEY_SERVER, v.stats.server());
        if let Some(server_extra_tags) = v.stats.server_extra_tags() {
            common_tags.add_static_tags(&server_extra_tags);
        }
        super::user::emit_traffic_stats_with_tags(
            client,
            &v.stats,
            &mut v.snap,
            &CLIENT_TRAFFIC_STATS_NAMES,
            &common_tags,
        );
        // use Arc instead of Weak here, as we should emit the final metrics before drop it
        Arc::strong_count(&v.stats) > 1
    });
    drop(io_stats_map);

    let mut upstream_io_stats_map = SITE_UPSTREAM_TRAFFIC_STATS_MAP.lock().unwrap();
    upstream_io_stats_map.retain(|v| {
        let mut common_tags = StatsdTagGroup::default();
        v.site.add_to(&mut common_tags, v.stats.stat_id());
        common_tags.add_tag(TAG_KEY_ESCAPER, v.stats.escaper());
        if let Some(escaper_extra_tags) = v.stats.escaper_extra_tags() {
            common_tags.add_static_tags(&escaper_extra_tags);
        }
        super::user::emit_upstream_traffic_stats_with_tags(
            client,
            &v.stats,
            &mut v.snap,
            &UPSTREAM_TRAFFIC_STATS_NAMES,
            &common_tags,
        );
        // use Arc instead of Weak here, as we should emit the final metrics before drop it
        Arc::strong_count(&v.stats) > 1
    });
    drop(upstream_io_stats_map);
}
