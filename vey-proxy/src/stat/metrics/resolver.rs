/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use vey_daemon::metrics::TAG_KEY_STAT_ID;
use vey_resolver::{
    ResolveQueryType, ResolverMemorySnapshot, ResolverQuerySnapshot, ResolverSnapshot,
};
use vey_statsd_client::{StatsdClient, StatsdTagGroup};
use vey_types::metrics::NodeName;
use vey_types::stats::{GlobalStatsMap, StatId};

use crate::resolve::ResolverStats;

const TAG_KEY_RESOLVER: &str = "resolver";
const TAG_KEY_RR_TYPE: &str = "rr_type";

const METRIC_NAME_QUERY_TOTAL: &str = "resolver.query.total";
const METRIC_NAME_QUERY_CACHED: &str = "resolver.query.cached";
const METRIC_NAME_QUERY_TRASHED: &str = "resolver.query.trashed";
const METRIC_NAME_QUERY_TIMEOUT: &str = "resolver.query.timeout";
const METRIC_NAME_QUERY_DRIVER: &str = "resolver.query.driver.total";
const METRIC_NAME_QUERY_DRIVER_TIMEOUT: &str = "resolver.query.driver.timeout";
const METRIC_NAME_QUERY_DRIVER_FAILED: &str = "resolver.query.driver.failed";
const METRIC_NAME_QUERY_SERVER_REFUSED: &str = "resolver.query.server.refused";
const METRIC_NAME_QUERY_SERVER_MALFORMED: &str = "resolver.query.server.malformed";
const METRIC_NAME_QUERY_SERVER_NOT_FOUND: &str = "resolver.query.server.not_found";
const METRIC_NAME_QUERY_SERVER_SERV_FAIL: &str = "resolver.query.server.serv_fail";
const METRIC_NAME_QUERY_SERVER_OTHER_CODE: &str = "resolver.query.server.other_code";
const METRIC_NAME_MEMORY_CACHE_CAPACITY: &str = "resolver.memory.cache.capacity";
const METRIC_NAME_MEMORY_CACHE_LENGTH: &str = "resolver.memory.cache.length";
const METRIC_NAME_MEMORY_DOING_CAPACITY: &str = "resolver.memory.doing.capacity";
const METRIC_NAME_MEMORY_DOING_LENGTH: &str = "resolver.memory.doing.length";
const METRIC_NAME_MEMORY_TRASH_CAPACITY: &str = "resolver.memory.trash.capacity";
const METRIC_NAME_MEMORY_TRASH_LENGTH: &str = "resolver.memory.trash.length";

type ResolverStatsValue = (Arc<ResolverStats>, ResolverSnapshot);

static RESOLVER_STATS_MAP: Mutex<GlobalStatsMap<ResolverStatsValue>> =
    Mutex::new(GlobalStatsMap::new());

trait ResolverMetricExt {
    fn add_resolver_tags(&mut self, resolver: &NodeName, stat_id: StatId);
}

impl ResolverMetricExt for StatsdTagGroup {
    fn add_resolver_tags(&mut self, resolver: &NodeName, stat_id: StatId) {
        let mut buffer = itoa::Buffer::new();
        let stat_id = buffer.format(stat_id.as_u64());
        self.add_tag(TAG_KEY_RESOLVER, resolver);
        self.add_tag(TAG_KEY_STAT_ID, stat_id);
    }
}

pub(in crate::stat) fn sync_stats() {
    let mut stats_map = RESOLVER_STATS_MAP.lock().unwrap();
    crate::resolve::foreach_resolver(|_, server| {
        let stats = server.get_stats();
        stats_map.get_or_insert_with(stats.stat_id(), || (stats, ResolverSnapshot::default()));
    });
}

pub(in crate::stat) fn emit_stats(client: &mut StatsdClient) {
    let mut stats_map = RESOLVER_STATS_MAP.lock().unwrap();
    stats_map.retain(|(stats, snap)| {
        emit_to_statsd(client, stats, snap);
        // use Arc instead of Weak here, as we should emit the final metrics before drop it
        Arc::strong_count(stats) > 1
    });
}

fn emit_to_statsd(client: &mut StatsdClient, stats: &ResolverStats, snap: &mut ResolverSnapshot) {
    let mut common_tags = StatsdTagGroup::default();
    common_tags.add_resolver_tags(stats.name(), stats.stat_id());

    let inner_stats = stats.inner().snapshot();

    emit_query_stats_to_statsd(
        client,
        &inner_stats.query_a,
        &mut snap.query_a,
        &common_tags,
        ResolveQueryType::A,
    );

    emit_query_stats_to_statsd(
        client,
        &inner_stats.query_aaaa,
        &mut snap.query_aaaa,
        &common_tags,
        ResolveQueryType::Aaaa,
    );

    emit_memory_stats_to_statsd(
        client,
        &inner_stats.memory_a,
        &common_tags,
        ResolveQueryType::A,
    );

    emit_memory_stats_to_statsd(
        client,
        &inner_stats.memory_aaaa,
        &common_tags,
        ResolveQueryType::Aaaa,
    );
}

fn emit_query_stats_to_statsd(
    client: &mut StatsdClient,
    stats: &ResolverQuerySnapshot,
    snap: &mut ResolverQuerySnapshot,
    common_tags: &StatsdTagGroup,
    rr_type: ResolveQueryType,
) {
    if stats.query_total == 0 && snap.query_total == 0 {
        return;
    }

    let rr_type = rr_type.as_str();

    let new_value = stats.query_total;
    let diff_value = new_value.wrapping_sub(snap.query_total);
    client
        .count_with_tags(METRIC_NAME_QUERY_TOTAL, diff_value, common_tags)
        .with_tag(TAG_KEY_RR_TYPE, rr_type)
        .send();
    snap.query_total = new_value;

    macro_rules! emit_query_stats_u64 {
        ($id:ident, $name:expr) => {
            let new_value = stats.$id;
            if new_value != 0 || snap.$id != 0 {
                let diff_value = new_value.wrapping_sub(snap.$id);
                client
                    .count_with_tags($name, diff_value, common_tags)
                    .with_tag(TAG_KEY_RR_TYPE, rr_type)
                    .send();
                snap.$id = new_value;
            }
        };
    }

    emit_query_stats_u64!(query_cached, METRIC_NAME_QUERY_CACHED);
    emit_query_stats_u64!(query_trashed, METRIC_NAME_QUERY_TRASHED);
    emit_query_stats_u64!(query_driver, METRIC_NAME_QUERY_DRIVER);
    emit_query_stats_u64!(query_timeout, METRIC_NAME_QUERY_TIMEOUT);
    emit_query_stats_u64!(driver_timeout, METRIC_NAME_QUERY_DRIVER_TIMEOUT);
    emit_query_stats_u64!(driver_failed, METRIC_NAME_QUERY_DRIVER_FAILED);
    emit_query_stats_u64!(server_refused, METRIC_NAME_QUERY_SERVER_REFUSED);
    emit_query_stats_u64!(server_malformed, METRIC_NAME_QUERY_SERVER_MALFORMED);
    emit_query_stats_u64!(server_not_found, METRIC_NAME_QUERY_SERVER_NOT_FOUND);
    emit_query_stats_u64!(server_serv_fail, METRIC_NAME_QUERY_SERVER_SERV_FAIL);
    emit_query_stats_u64!(server_other_code, METRIC_NAME_QUERY_SERVER_OTHER_CODE);
}

fn emit_memory_stats_to_statsd(
    client: &mut StatsdClient,
    snap: &ResolverMemorySnapshot,
    common_tags: &StatsdTagGroup,
    rr_type: ResolveQueryType,
) {
    macro_rules! emit_field {
        ($field:ident, $name:expr) => {
            client
                .gauge_with_tags($name, snap.$field, common_tags)
                .with_tag(TAG_KEY_RR_TYPE, rr_type)
                .send();
        };
    }

    emit_field!(cap_cache, METRIC_NAME_MEMORY_CACHE_CAPACITY);
    emit_field!(len_cache, METRIC_NAME_MEMORY_CACHE_LENGTH);
    emit_field!(cap_doing, METRIC_NAME_MEMORY_DOING_CAPACITY);
    emit_field!(len_doing, METRIC_NAME_MEMORY_DOING_LENGTH);
    emit_field!(cap_trash, METRIC_NAME_MEMORY_TRASH_CAPACITY);
    emit_field!(len_trash, METRIC_NAME_MEMORY_TRASH_LENGTH);
}
