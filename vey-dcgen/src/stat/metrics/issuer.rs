/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, Mutex};

use vey_daemon::metrics::TAG_KEY_QUANTILE;
use vey_histogram::HistogramStats;
use vey_statsd_client::StatsdClient;

const METRIC_NAME_ISSUER_REFRESH_TOTAL: &str = "issuer.refresh.total";
const METRIC_NAME_ISSUER_REFRESH_OK: &str = "issuer.refresh.ok";
const METRIC_NAME_ISSUER_REQUEST_TOTAL: &str = "issuer.request.total";
const METRIC_NAME_ISSUER_REQUEST_OK: &str = "issuer.request.ok";
const METRIC_NAME_ISSUER_REQUEST_DURATION: &str = "issuer.request.duration";

static ISSUER_SNAPSHOT: Mutex<IssuerSnapshot> = Mutex::new(IssuerSnapshot {
    refresh_total: 0,
    refresh_ok: 0,
    request_total: 0,
    request_ok: 0,
});
static DURATION_STATS: Mutex<Option<Arc<HistogramStats>>> = Mutex::new(None);

#[derive(Default)]
struct IssuerSnapshot {
    refresh_total: u64,
    refresh_ok: u64,
    request_total: u64,
    request_ok: u64,
}

pub(crate) fn push_duration_stats(stats: Arc<HistogramStats>) {
    *DURATION_STATS.lock().unwrap() = Some(stats);
}

pub(in crate::stat) fn emit_stats(client: &mut StatsdClient) {
    let stats = crate::issuer::issuer_stats();
    let mut snap = ISSUER_SNAPSHOT.lock().unwrap();

    macro_rules! emit_count {
        ($get:ident, $snap:ident, $name:expr) => {
            let new_value = stats.$get();
            let diff_value = new_value.wrapping_sub(snap.$snap);
            client.count($name, diff_value).send();
            snap.$snap = new_value;
        };
    }

    emit_count!(
        refresh_total,
        refresh_total,
        METRIC_NAME_ISSUER_REFRESH_TOTAL
    );
    emit_count!(refresh_ok, refresh_ok, METRIC_NAME_ISSUER_REFRESH_OK);
    emit_count!(
        request_total,
        request_total,
        METRIC_NAME_ISSUER_REQUEST_TOTAL
    );
    emit_count!(request_ok, request_ok, METRIC_NAME_ISSUER_REQUEST_OK);
    drop(snap);

    let duration_stats = DURATION_STATS.lock().unwrap();
    if let Some(stats) = duration_stats.as_ref() {
        emit_duration_stats(client, stats);
    }
}

fn emit_duration_stats(client: &mut StatsdClient, stats: &HistogramStats) {
    stats.foreach_stat(|_, qs, v| {
        if v > 0_f64 {
            client
                .gauge_float(METRIC_NAME_ISSUER_REQUEST_DURATION, v)
                .with_tag(TAG_KEY_QUANTILE, qs)
                .send();
        }
    });
}
