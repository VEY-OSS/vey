/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use vey_histogram::{HistogramRecorder, KeepingHistogram};
use vey_statsd_client::StatsdClient;
use vey_std_ext::time::DurationExt;

use crate::target::BenchHistogram;

pub(crate) struct KeylessHistogram {
    total_time: KeepingHistogram<u64>,
    conn_used_times: KeepingHistogram<u64>,
}

impl KeylessHistogram {
    pub(crate) fn new() -> (Self, KeylessHistogramRecorder) {
        let (total_time_h, total_time_r) = KeepingHistogram::new();
        let (conn_used_times_h, conn_used_times_r) = KeepingHistogram::new();
        let h = KeylessHistogram {
            total_time: total_time_h,
            conn_used_times: conn_used_times_h,
        };
        let r = KeylessHistogramRecorder {
            total_time: total_time_r,
            conn_used_times: conn_used_times_r,
        };
        (h, r)
    }
}

impl BenchHistogram for KeylessHistogram {
    fn refresh(&mut self) {
        self.total_time.refresh().unwrap();
        self.conn_used_times.refresh().unwrap();
    }

    fn emit(&self, client: &mut StatsdClient) {
        self.emit_histogram(client, self.total_time.inner(), "keyless.time.total");
    }

    fn summary(&self) {
        Self::summary_histogram_title("# Connection Used Times:");
        Self::summary_data_line("Req/Conn:", self.conn_used_times.inner());
        Self::summary_histogram_title("# Duration Times");
        Self::summary_duration_line("Total:", self.total_time.inner());
        Self::summary_newline();
        Self::summary_total_percentage(self.total_time.inner());
    }
}

#[derive(Clone)]
pub(crate) struct KeylessHistogramRecorder {
    total_time: HistogramRecorder<u64>,
    conn_used_times: HistogramRecorder<u64>,
}

impl KeylessHistogramRecorder {
    pub(crate) fn record_total_time(&mut self, dur: Duration) {
        let _ = self.total_time.record(dur.as_nanos_u64());
    }

    pub(crate) fn record_conn_used_times(&mut self, count: u64) {
        let _ = self.conn_used_times.record(count);
    }
}
