/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use hdrhistogram::Histogram;

use crate::{
    HistogramMetricsConfig, HistogramStats, KeepingHistogram, Quantile, RotatingHistogram,
};

#[test]
fn keeping_histogram_refresh_records() {
    let (mut hist, recorder) = KeepingHistogram::new();
    recorder.record(42u64).unwrap();
    recorder.record(100u64).unwrap();
    hist.refresh().unwrap();

    let inner = hist.inner();
    assert_eq!(inner.len(), 2);
    assert_eq!(inner.min(), 42);
    assert_eq!(inner.max(), 100);
}

#[test]
fn histogram_stats_update_and_foreach() {
    let mut hist = Histogram::<u64>::new(3).unwrap();
    for v in [10, 20, 30, 40, 50] {
        hist.record(v).unwrap();
    }

    let stats = HistogramStats::with_quantiles(&[Quantile::PCT50, Quantile::PCT99]);
    stats.update(&hist);

    let mut collected = Vec::new();
    stats.foreach_stat(|q, name, value| {
        collected.push((q, name.to_string(), value));
    });

    assert!(collected.iter().any(|(_, n, _)| n == "min"));
    assert!(collected.iter().any(|(_, n, _)| n == "max"));
    assert!(collected.iter().any(|(_, n, _)| n == "mean"));
    assert!(collected.iter().any(|(q, _, _)| *q == Some(0.50)));
    assert!(collected.iter().any(|(q, _, _)| *q == Some(0.99)));
}

#[test]
fn histogram_metrics_config_defaults() {
    let config = HistogramMetricsConfig::default();
    assert_eq!(config.rotate_interval(), Duration::from_secs(4));
}

#[test]
fn histogram_metrics_config_custom_quantiles() {
    let mut config = HistogramMetricsConfig::with_rotate(Duration::from_secs(2));
    let mut quantiles = BTreeSet::new();
    quantiles.insert(Quantile::PCT95);
    config.set_quantile_list(quantiles);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let (_recorder, stats) = config.build_spawned::<u64>(Some(rt.handle().clone()));
    assert!(Arc::strong_count(&stats) >= 1);
}

#[test]
fn rotating_histogram_recorder() {
    let (_hist, recorder) = RotatingHistogram::<u64>::new(Duration::from_secs(1));
    recorder.record(7u64).unwrap();
    recorder.record(9u64).unwrap();
}
