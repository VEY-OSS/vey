/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::sync::Mutex;

use tokio::runtime::RuntimeMetrics;

use vey_statsd_client::{StatsdClient, StatsdTagGroup};
use vey_types::stats::StatId;

use crate::metrics::TAG_KEY_STAT_ID;

const TAG_KEY_RUNTIME_ID: &str = "runtime_id";

const METRIC_NAME_RUNTIME_TOKIO_ALIVE_TASKS: &str = "runtime.tokio.alive_tasks";
const METRIC_NAME_RUNTIME_TOKIO_GLOBAL_QUEUE_DEPTH: &str = "runtime.tokio.global_queue_depth";

static TOKIO_STATS_VEC: Mutex<Vec<TokioStatsValue>> = Mutex::new(Vec::new());

struct TokioStatsValue {
    stat_id: StatId,
    runtime_id: String,
    stats: RuntimeMetrics,
}

pub fn add_tokio_stats(stats: RuntimeMetrics, id: String) {
    let value = TokioStatsValue {
        stat_id: StatId::new_unique(),
        runtime_id: id,
        stats,
    };
    let mut tokio_stats_vec = TOKIO_STATS_VEC.lock().unwrap();
    tokio_stats_vec.push(value);
}

pub fn emit_stats(client: &mut StatsdClient) {
    #[cfg(feature = "jemalloc")]
    emit_jemalloc_stats(client);

    let mut tokio_stats_vec = TOKIO_STATS_VEC.lock().unwrap();
    for v in tokio_stats_vec.iter_mut() {
        emit_tokio_stats(client, v);
    }
}

fn emit_tokio_stats(client: &mut StatsdClient, v: &mut TokioStatsValue) {
    let mut common_tags = StatsdTagGroup::default();
    let mut buffer = itoa::Buffer::new();
    let stat_id = buffer.format(v.stat_id.as_u64());
    common_tags.add_tag(TAG_KEY_STAT_ID, stat_id);
    common_tags.add_tag(TAG_KEY_RUNTIME_ID, &v.runtime_id);

    client
        .gauge_with_tags(
            METRIC_NAME_RUNTIME_TOKIO_ALIVE_TASKS,
            v.stats.num_alive_tasks(),
            &common_tags,
        )
        .send();
    client
        .gauge_with_tags(
            METRIC_NAME_RUNTIME_TOKIO_GLOBAL_QUEUE_DEPTH,
            v.stats.global_queue_depth(),
            &common_tags,
        )
        .send();
}

#[cfg(feature = "jemalloc")]
fn emit_jemalloc_stats(client: &mut StatsdClient) {
    use std::sync::LazyLock;
    use vey_jemalloc::stats::JemallocStatsEntry;

    const METRIC_NAME_RUNTIME_JEMALLOC_ACTIVE: &str = "runtime.jemalloc.approximate_active";
    static ACTIVE_STATS: LazyLock<Option<JemallocStatsEntry<usize>>> =
        LazyLock::new(vey_jemalloc::stats::approximate_active);

    if let Some(stats) = ACTIVE_STATS.as_ref()
        && let Some(value) = stats.value()
    {
        client
            .gauge(METRIC_NAME_RUNTIME_JEMALLOC_ACTIVE, value)
            .send();
    }
}
