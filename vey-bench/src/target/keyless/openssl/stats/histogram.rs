/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::time::Duration;

use vey_histogram::{HistogramRecorder, KeepingHistogram};
use vey_statsd_client::StatsdClient;
use vey_std_ext::time::DurationExt;

use crate::summary::{hist_row_from_duration, print_hist_table, print_pct_table};
use crate::target::BenchHistogram;

pub(crate) struct KeylessHistogram {
    total_time: KeepingHistogram<u64>,
}

impl KeylessHistogram {
    pub(crate) fn new() -> (Self, KeylessHistogramRecorder) {
        let (h, r) = KeepingHistogram::new();
        (
            KeylessHistogram { total_time: h },
            KeylessHistogramRecorder { total_time: r },
        )
    }
}

impl BenchHistogram for KeylessHistogram {
    fn refresh(&mut self) {
        self.total_time.refresh().unwrap();
    }

    fn emit(&self, client: &mut StatsdClient) {
        self.emit_histogram(client, self.total_time.inner(), "keyless.time.total");
    }

    fn summary(&self) {
        let total_time = self.total_time.inner();
        print_hist_table(
            "# Duration Times",
            &[hist_row_from_duration("Total", total_time)],
        );
        print_pct_table(total_time);
    }
}

#[derive(Clone)]
pub(crate) struct KeylessHistogramRecorder {
    total_time: HistogramRecorder<u64>,
}

impl KeylessHistogramRecorder {
    pub(crate) fn record_total_time(&mut self, dur: Duration) {
        let _ = self.total_time.record(dur.as_nanos_u64());
    }
}
