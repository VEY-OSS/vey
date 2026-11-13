/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use arc_swap::ArcSwapOption;

use vey_histogram::HistogramRecorder;

use crate::config::issuer::OpensslIssuerConfig;

static DURATION_RECORDER: ArcSwapOption<HistogramRecorder<u64>> = ArcSwapOption::const_empty();

pub(super) fn load_duration_stats(config: &OpensslIssuerConfig) {
    let (recorder, stats) = config.duration_stats.build_spawned(None);
    DURATION_RECORDER.store(Some(Arc::new(recorder)));
    crate::stat::metrics::issuer::push_duration_stats(stats);
}

pub(crate) fn issuer_duration_recorder() -> Option<HistogramRecorder<u64>> {
    DURATION_RECORDER
        .load_full()
        .map(|recorder| recorder.as_ref().clone())
}

#[derive(Default)]
pub(crate) struct IssuerStats {
    refresh_total: AtomicU64,
    refresh_ok: AtomicU64,
    request_total: AtomicU64,
    request_ok: AtomicU64,
}

macro_rules! impl_for_field {
    ($add:ident, $get:ident, $field:ident) => {
        pub(super) fn $add(&self) {
            self.$field.fetch_add(1, Ordering::Relaxed);
        }

        pub(crate) fn $get(&self) -> u64 {
            self.$field.load(Ordering::Relaxed)
        }
    };
}

impl IssuerStats {
    impl_for_field!(add_refresh_total, refresh_total, refresh_total);
    impl_for_field!(add_refresh_ok, refresh_ok, refresh_ok);
    impl_for_field!(add_request_total, request_total, request_total);
    impl_for_field!(add_request_ok, request_ok, request_ok);
}

static ISSUER_STATS: LazyLock<IssuerStats> = LazyLock::new(IssuerStats::default);

pub(crate) fn issuer_stats() -> &'static IssuerStats {
    &ISSUER_STATS
}
