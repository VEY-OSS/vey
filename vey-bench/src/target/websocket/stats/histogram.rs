/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::time::Duration;

use vey_histogram::{HistogramRecorder, KeepingHistogram};
use vey_statsd_client::StatsdClient;
use vey_std_ext::time::DurationExt;

use crate::target::BenchHistogram;

pub(crate) struct WebsocketHistogram {
    total_time: KeepingHistogram<u64>,
}

impl WebsocketHistogram {
    pub(crate) fn new() -> (Self, WebsocketHistogramRecorder) {
        let (total_time_h, total_time_r) = KeepingHistogram::new();
        let h = WebsocketHistogram {
            total_time: total_time_h,
        };
        let r = WebsocketHistogramRecorder {
            total_time: total_time_r,
        };
        (h, r)
    }
}

impl BenchHistogram for WebsocketHistogram {
    fn refresh(&mut self) {
        self.total_time.refresh().unwrap();
    }

    fn emit(&self, client: &mut StatsdClient) {
        self.emit_histogram(client, self.total_time.inner(), "websocket.time.total");
    }

    fn summary(&self) {
        Self::summary_histogram_title("# Duration Times");
        Self::summary_duration_line("Total:", self.total_time.inner());
        Self::summary_newline();
        Self::summary_total_percentage(self.total_time.inner());
    }
}

#[derive(Clone)]
pub(crate) struct WebsocketHistogramRecorder {
    total_time: HistogramRecorder<u64>,
}

impl WebsocketHistogramRecorder {
    pub(crate) fn record_total_time(&mut self, dur: Duration) {
        let _ = self.total_time.record(dur.as_nanos_u64());
    }
}
