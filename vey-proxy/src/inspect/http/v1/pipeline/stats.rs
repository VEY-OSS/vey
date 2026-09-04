/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

pub(crate) struct PipelineStats {
    total_task: AtomicU64,
    alive_task: AtomicI32,
}

impl Default for PipelineStats {
    fn default() -> Self {
        PipelineStats {
            total_task: AtomicU64::new(0),
            alive_task: AtomicI32::new(0),
        }
    }
}

impl PipelineStats {
    #[must_use]
    pub(super) fn add_task(self: &Arc<Self>) -> PipelineTaskGuard {
        self.total_task.fetch_add(1, Ordering::Relaxed);
        self.alive_task.fetch_add(1, Ordering::Relaxed);
        PipelineTaskGuard(Arc::clone(self))
    }

    pub(super) fn get_alive_task(&self) -> i32 {
        self.alive_task.load(Ordering::Relaxed)
    }
}

pub(crate) struct PipelineTaskGuard(Arc<PipelineStats>);

impl Drop for PipelineTaskGuard {
    fn drop(&mut self) {
        self.0.alive_task.fetch_sub(1, Ordering::Relaxed);
    }
}
