/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use arc_swap::ArcSwapOption;

use vey_histogram::{HistogramRecorder, HistogramStats};

use crate::config::issuer::OpensslIssuerConfig;

static RUNTIME_CONFIG: ArcSwapOption<OpensslIssuerConfig> = ArcSwapOption::const_empty();
static DURATION_RECORDER: ArcSwapOption<HistogramRecorder<u64>> = ArcSwapOption::const_empty();
static DURATION_STATS: ArcSwapOption<HistogramStats> = ArcSwapOption::const_empty();

pub(crate) fn get_config() -> Option<Arc<OpensslIssuerConfig>> {
    RUNTIME_CONFIG.load_full()
}

pub(super) fn publish() -> bool {
    let Some(config) = crate::config::issuer::get_config() else {
        return false;
    };
    let (recorder, stats) = config.duration_stats.build_spawned(None);
    DURATION_RECORDER.store(Some(Arc::new(recorder)));
    DURATION_STATS.store(Some(stats));
    RUNTIME_CONFIG.store(Some(Arc::new(config)));
    true
}

pub(crate) fn duration_stats() -> Option<Arc<HistogramStats>> {
    DURATION_STATS.load_full()
}

pub(crate) fn duration_recorder() -> Option<HistogramRecorder<u64>> {
    DURATION_RECORDER
        .load_full()
        .map(|recorder| recorder.as_ref().clone())
}
