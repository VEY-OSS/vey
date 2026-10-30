/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use vey_daemon::listen::{ListenSnapshot, ListenStats};
use vey_daemon::metrics::{TAG_KEY_SERVER, emit_listen_stats};
use vey_statsd_client::{StatsdClient, StatsdTagGroup};
use vey_types::stats::GlobalStatsMap;

use crate::serve::ServerStats;

static SERVER_STATS_MAP: Mutex<GlobalStatsMap<Arc<ServerStats>>> =
    Mutex::new(GlobalStatsMap::new());
static LISTEN_STATS_MAP: Mutex<GlobalStatsMap<(Arc<ListenStats>, ListenSnapshot)>> =
    Mutex::new(GlobalStatsMap::new());

pub(super) fn emit_stats(client: &mut StatsdClient) {
    let mut server_stats = Vec::new();
    let mut listen_stats = Vec::new();
    crate::serve::foreach_online(|server| {
        server_stats.push(server.server_stats());
        listen_stats.push(server.listen_stats());
    });

    let mut server_stats_map = SERVER_STATS_MAP.lock().unwrap();
    for stats in server_stats {
        let stat_id = stats.stat_id();
        server_stats_map.get_or_insert_with(stat_id, || stats);
    }
    server_stats_map.retain(|stats| {
        emit_server_stats(client, stats);
        Arc::strong_count(stats) > 1
    });
    drop(server_stats_map);

    let mut listen_stats_map = LISTEN_STATS_MAP.lock().unwrap();
    for stats in listen_stats {
        let stat_id = stats.stat_id();
        listen_stats_map.get_or_insert_with(stat_id, || (stats, ListenSnapshot::default()));
    }
    listen_stats_map.retain(|(stats, snap)| {
        emit_listen_stats(client, stats, snap);
        Arc::strong_count(stats) > 1
    });
}

fn emit_server_stats(client: &mut StatsdClient, stats: &Arc<ServerStats>) {
    let mut tags = StatsdTagGroup::default();
    tags.add_tag(TAG_KEY_SERVER, stats.name());

    macro_rules! emit_count {
        ($take:ident, $name:literal) => {
            let v = stats.$take();
            client
                .count_with_tags(concat!("server.", $name), v, &tags)
                .send();
        };
    }

    emit_count!(take_request_total, "request_total");
    emit_count!(take_request_invalid, "request_invalid");
    emit_count!(take_lookup_not_found, "lookup_not_found");
    emit_count!(take_response_total, "response_total");
    emit_count!(take_response_fail, "response_fail");

    client
        .gauge_with_tags("server.request_alive", stats.alive_count(), &tags)
        .send();
}
