/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use hdrhistogram::Histogram;
use serde_json::Value;

use crate::report::{JsonObject, hist_snapshot, insert, json_f64, json_u64, json_usize, keys};
use crate::summary::{
    KvRow, hist_row_from_data, print_hist_table, print_kv_section, print_split_line,
};

static GLOBAL_STATE: GlobalState = GlobalState::new(None, 0);

pub(super) fn global_state() -> &'static GlobalState {
    &GLOBAL_STATE
}

pub(super) fn mark_force_quit() {
    GLOBAL_STATE.mark_force_quit();
}

pub(super) fn init_global_state(requests: Option<usize>, log_error_count: usize) {
    GLOBAL_STATE
        .check_total
        .store(requests.is_some(), Ordering::Relaxed);
    GLOBAL_STATE
        .total_left
        .store(requests.unwrap_or_default(), Ordering::Relaxed);
    GLOBAL_STATE
        .log_error_left
        .store(log_error_count, Ordering::Relaxed);
}

pub(super) struct GlobalState {
    check_total: AtomicBool,
    force_quit: AtomicBool,
    total_left: AtomicUsize,
    total_passed: AtomicUsize,
    total_failed: AtomicUsize,
    log_error_left: AtomicUsize,
    request_id: AtomicUsize,
}

impl Default for GlobalState {
    fn default() -> Self {
        GlobalState::new(None, 0)
    }
}

impl GlobalState {
    pub(super) const fn new(requests: Option<usize>, log_error_count: usize) -> Self {
        let total_left = match requests {
            Some(n) => AtomicUsize::new(n),
            None => AtomicUsize::new(0),
        };
        GlobalState {
            check_total: AtomicBool::new(requests.is_some()),
            force_quit: AtomicBool::new(false),
            total_left,
            total_passed: AtomicUsize::new(0),
            total_failed: AtomicUsize::new(0),
            log_error_left: AtomicUsize::new(log_error_count),
            request_id: AtomicUsize::new(0),
        }
    }

    fn mark_force_quit(&self) {
        self.force_quit.store(true, Ordering::Relaxed);
    }

    pub(super) fn fetch_request(&self) -> Option<usize> {
        if self.force_quit.load(Ordering::Relaxed) {
            return None;
        }

        if self.check_total.load(Ordering::Relaxed) {
            let mut curr = self.total_left.load(Ordering::Acquire);
            loop {
                if curr == 0 {
                    return None;
                }

                match self.total_left.compare_exchange_weak(
                    curr,
                    curr - 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(actual) => curr = actual,
                }
            }
        }

        Some(self.request_id.fetch_add(1, Ordering::Relaxed))
    }

    pub(super) fn check_log_error(&self) -> bool {
        let mut curr = self.log_error_left.load(Ordering::Acquire);
        loop {
            if curr == 0 {
                return false;
            }

            match self.log_error_left.compare_exchange_weak(
                curr,
                curr - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(actual) => curr = actual,
            }
        }
    }

    pub(super) fn add_passed(&self) {
        self.total_passed.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn add_failed(&self) {
        self.total_failed.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn all_succeeded(&self) -> bool {
        self.total_failed.load(Ordering::Relaxed) == 0
    }

    pub(super) fn summary(&self, total_time: Duration, distribution: Option<&Histogram<u64>>) {
        let passed = self.total_passed.load(Ordering::Relaxed);
        let mut rows = vec![
            KvRow::new("Time taken for tests", format!("{total_time:?}")),
            KvRow::new("Complete requests", passed),
        ];

        let failed = self.total_failed.load(Ordering::Relaxed);
        if failed > 0 {
            rows.push(KvRow::new("Failed requests", failed));
        }

        let left = self.total_left.load(Ordering::Relaxed);
        if left > 0 {
            rows.push(KvRow::new("Left requests", left));
        }

        rows.push(KvRow::new(
            "Requests per second",
            format!(
                "{:.3} [#/sec] (mean)",
                passed as f64 / total_time.as_secs_f64()
            ),
        ));
        print_split_line();
        print_kv_section("", &rows);

        let Some(distribution) = distribution else {
            return;
        };
        print_hist_table(
            "Requests distribution",
            &[hist_row_from_data("", distribution)],
        );
    }

    pub(super) fn json_report(
        &self,
        total_time: Duration,
        distribution: Option<&Histogram<u64>>,
    ) -> Value {
        let passed = self.total_passed.load(Ordering::Relaxed);
        let failed = self.total_failed.load(Ordering::Relaxed);
        let left = self.total_left.load(Ordering::Relaxed);
        let mut obj = JsonObject::new();
        insert(
            &mut obj,
            keys::TOTAL_TIME_NS,
            json_u64(total_time.as_nanos() as u64),
        );
        insert(&mut obj, keys::COMPLETE_REQUESTS, json_usize(passed));
        insert(&mut obj, keys::FAILED_REQUESTS, json_usize(failed));
        insert(&mut obj, keys::LEFT_REQUESTS, json_usize(left));
        insert(
            &mut obj,
            keys::REQUESTS_PER_SEC,
            json_f64(passed as f64 / total_time.as_secs_f64()),
        );
        if let Some(distribution) = distribution {
            insert(
                &mut obj,
                keys::REQUESTS_DISTRIBUTION,
                hist_snapshot(distribution),
            );
        }
        Value::Object(obj)
    }
}
