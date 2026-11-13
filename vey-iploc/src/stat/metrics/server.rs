/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use vey_daemon::listen::{ListenSnapshot, ListenStats};
use vey_daemon::metrics::ServerMetricExt;
use vey_statsd_client::{StatsdClient, StatsdTagGroup};
use vey_types::stats::GlobalStatsMap;

use crate::serve::ServerStats;

const METRIC_NAME_SERVER_REQUEST_TOTAL: &str = "server.request.total";
const METRIC_NAME_SERVER_REQUEST_INVALID: &str = "server.request.invalid";
const METRIC_NAME_SERVER_LOOKUP_NOT_FOUND: &str = "server.lookup.not_found";
const METRIC_NAME_SERVER_REQUEST_ALIVE: &str = "server.request.alive";
const METRIC_NAME_SERVER_RESPONSE_TOTAL: &str = "server.response.total";
const METRIC_NAME_SERVER_RESPONSE_FAIL: &str = "server.response.fail";

type ServerStatsValue = (Arc<ServerStats>, ServerSnapshot);
type ListenStatsValue = (Arc<ListenStats>, ListenSnapshot);

static SERVER_STATS_MAP: Mutex<GlobalStatsMap<ServerStatsValue>> =
    Mutex::new(GlobalStatsMap::new());
static LISTEN_STATS_MAP: Mutex<GlobalStatsMap<ListenStatsValue>> =
    Mutex::new(GlobalStatsMap::new());

#[derive(Default)]
struct ServerSnapshot {
    request_total: u64,
    request_invalid: u64,
    lookup_not_found: u64,
    response_total: u64,
    response_fail: u64,
}

pub(in crate::stat) fn sync_stats() {
    let mut server_stats_map = SERVER_STATS_MAP.lock().unwrap();
    crate::serve::foreach_online(|server| {
        let stats = server.server_stats();
        server_stats_map.get_or_insert_with(stats.stat_id(), || (stats, ServerSnapshot::default()));
    });
    drop(server_stats_map);

    let mut listen_stats_map = LISTEN_STATS_MAP.lock().unwrap();
    crate::serve::foreach_online(|server| {
        let stats = server.listen_stats();
        listen_stats_map.get_or_insert_with(stats.stat_id(), || (stats, ListenSnapshot::default()));
    });
    drop(listen_stats_map);
}

pub(in crate::stat) fn emit_stats(client: &mut StatsdClient) {
    let mut server_stats_map = SERVER_STATS_MAP.lock().unwrap();
    server_stats_map.retain(|(stats, snap)| {
        emit_server_stats(client, stats, snap);
        // use Arc instead of Weak here, as we should emit the final metrics before drop it
        Arc::strong_count(stats) > 1
    });
    drop(server_stats_map);

    let mut listen_stats_map = LISTEN_STATS_MAP.lock().unwrap();
    listen_stats_map.retain(|(stats, snap)| {
        vey_daemon::metrics::emit_listen_stats(client, stats, snap);
        // use Arc instead of Weak here, as we should emit the final metrics before drop it
        Arc::strong_count(stats) > 1
    });
}

fn emit_server_stats(
    client: &mut StatsdClient,
    stats: &Arc<ServerStats>,
    snap: &mut ServerSnapshot,
) {
    let mut common_tags = StatsdTagGroup::default();
    common_tags.add_server_tags(stats.name(), stats.is_online(), stats.stat_id());

    macro_rules! emit_count {
        ($get:ident, $snap:ident, $name:expr) => {
            let new_value = stats.$get();
            let diff_value = new_value.wrapping_sub(snap.$snap);
            client
                .count_with_tags($name, diff_value, &common_tags)
                .send();
            snap.$snap = new_value;
        };
    }

    emit_count!(
        request_total,
        request_total,
        METRIC_NAME_SERVER_REQUEST_TOTAL
    );
    emit_count!(
        request_invalid,
        request_invalid,
        METRIC_NAME_SERVER_REQUEST_INVALID
    );
    emit_count!(
        lookup_not_found,
        lookup_not_found,
        METRIC_NAME_SERVER_LOOKUP_NOT_FOUND
    );
    emit_count!(
        response_total,
        response_total,
        METRIC_NAME_SERVER_RESPONSE_TOTAL
    );
    emit_count!(
        response_fail,
        response_fail,
        METRIC_NAME_SERVER_RESPONSE_FAIL
    );

    client
        .gauge_with_tags(
            METRIC_NAME_SERVER_REQUEST_ALIVE,
            stats.alive_count(),
            &common_tags,
        )
        .send();
}
