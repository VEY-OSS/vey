/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use vey_io_ext::{
    ArcLimitedReaderStats, ArcLimitedWriterStats, LimitedReaderStats, LimitedWriterStats,
};

use super::HttpGuardServerStats;

pub(crate) struct HttpGuardPipelineStats {
    total_task: AtomicU64,
    alive_task: AtomicI32,
}

impl Default for HttpGuardPipelineStats {
    fn default() -> Self {
        HttpGuardPipelineStats {
            total_task: AtomicU64::new(0),
            alive_task: AtomicI32::new(0),
        }
    }
}

impl HttpGuardPipelineStats {
    #[must_use]
    pub(super) fn add_task(self: &Arc<Self>) -> HttpGuardPipelineTaskGuard {
        self.total_task.fetch_add(1, Ordering::Relaxed);
        self.alive_task.fetch_add(1, Ordering::Relaxed);
        HttpGuardPipelineTaskGuard(Arc::clone(self))
    }

    pub(super) fn get_alive_task(&self) -> i32 {
        self.alive_task.load(Ordering::Relaxed)
    }
}

pub(crate) struct HttpGuardPipelineTaskGuard(Arc<HttpGuardPipelineStats>);

impl Drop for HttpGuardPipelineTaskGuard {
    fn drop(&mut self) {
        self.0.alive_task.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct HttpGuardCltWrapperStats {
    server: Arc<HttpGuardServerStats>,
}

impl HttpGuardCltWrapperStats {
    pub(crate) fn new_for_reader(server: &Arc<HttpGuardServerStats>) -> ArcLimitedReaderStats {
        let s = HttpGuardCltWrapperStats {
            server: Arc::clone(server),
        };
        Arc::new(s)
    }

    pub(crate) fn new_for_writer(server: &Arc<HttpGuardServerStats>) -> ArcLimitedWriterStats {
        let s = HttpGuardCltWrapperStats {
            server: Arc::clone(server),
        };
        Arc::new(s)
    }
}

impl LimitedReaderStats for HttpGuardCltWrapperStats {
    fn add_read_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_http.add_in_bytes(size);
    }
}

impl LimitedWriterStats for HttpGuardCltWrapperStats {
    fn add_write_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_http.add_out_bytes(size);
    }
}
