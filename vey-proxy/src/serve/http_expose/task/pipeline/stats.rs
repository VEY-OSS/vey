/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use vey_io_ext::{
    ArcLimitedReaderStats, ArcLimitedWriterStats, LimitedReaderStats, LimitedWriterStats,
};

use super::HttpExposeServerStats;

pub(crate) struct HttpExposePipelineStats {
    total_task: AtomicU64,
    alive_task: AtomicI32,
}

impl Default for HttpExposePipelineStats {
    fn default() -> Self {
        HttpExposePipelineStats {
            total_task: AtomicU64::new(0),
            alive_task: AtomicI32::new(0),
        }
    }
}

impl HttpExposePipelineStats {
    #[must_use]
    pub(super) fn add_task(self: &Arc<Self>) -> HttpExposePipelineTaskGuard {
        self.total_task.fetch_add(1, Ordering::Relaxed);
        self.alive_task.fetch_add(1, Ordering::Relaxed);
        HttpExposePipelineTaskGuard(Arc::clone(self))
    }

    pub(super) fn get_alive_task(&self) -> i32 {
        self.alive_task.load(Ordering::Relaxed)
    }
}

pub(crate) struct HttpExposePipelineTaskGuard(Arc<HttpExposePipelineStats>);

impl Drop for HttpExposePipelineTaskGuard {
    fn drop(&mut self) {
        self.0.alive_task.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct HttpExposeCltWrapperStats {
    server: Arc<HttpExposeServerStats>,
}

impl HttpExposeCltWrapperStats {
    pub(crate) fn new_for_reader(server: &Arc<HttpExposeServerStats>) -> ArcLimitedReaderStats {
        let s = HttpExposeCltWrapperStats {
            server: Arc::clone(server),
        };
        Arc::new(s)
    }

    pub(crate) fn new_for_writer(server: &Arc<HttpExposeServerStats>) -> ArcLimitedWriterStats {
        let s = HttpExposeCltWrapperStats {
            server: Arc::clone(server),
        };
        Arc::new(s)
    }
}

impl LimitedReaderStats for HttpExposeCltWrapperStats {
    fn add_read_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_http.add_in_bytes(size);
    }
}

impl LimitedWriterStats for HttpExposeCltWrapperStats {
    fn add_write_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_http.add_out_bytes(size);
    }
}
