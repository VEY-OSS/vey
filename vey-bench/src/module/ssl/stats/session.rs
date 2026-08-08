/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::report::tls_session_object;
use crate::summary::{KvRow, print_kv_section};

#[derive(Default)]
pub(crate) struct SslSessionStats {
    total: AtomicU64,
    reused: AtomicU64,
}

impl SslSessionStats {
    #[inline]
    pub(crate) fn add_total(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub(crate) fn add_reused(&self) {
        self.reused.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn summary(&self, prefix: &'static str) {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return;
        }

        let session_reused = self.reused.load(Ordering::Relaxed);
        print_kv_section(
            &format!("# {prefix} Session"),
            &[
                KvRow::new("Reused Count", session_reused),
                KvRow::new(
                    "Reuse Ratio",
                    format!("{:.2}%", (session_reused as f64 / total as f64) * 100.0),
                ),
            ],
        );
    }

    pub(crate) fn json_report(&self) -> Option<Value> {
        let total = self.total.load(Ordering::Relaxed);
        let reused = self.reused.load(Ordering::Relaxed);
        tls_session_object(total, reused)
    }
}
