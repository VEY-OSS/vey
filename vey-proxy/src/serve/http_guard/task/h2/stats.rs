/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use vey_io_ext::{LimitedReaderStats, LimitedWriterStats};

use super::super::HttpGuardServerStats;
use crate::auth::UserTrafficStats;

pub(crate) struct H2ConcurrencyStats {
    total_task: AtomicU64,
    alive_task: AtomicI32,
}

impl Default for H2ConcurrencyStats {
    fn default() -> Self {
        H2ConcurrencyStats {
            total_task: AtomicU64::new(0),
            alive_task: AtomicI32::new(0),
        }
    }
}

impl H2ConcurrencyStats {
    #[must_use]
    pub(super) fn add_task(self: &Arc<Self>) -> H2ConcurrencyTaskGuard {
        self.total_task.fetch_add(1, Ordering::Relaxed);
        self.alive_task.fetch_add(1, Ordering::Release);
        H2ConcurrencyTaskGuard(Arc::clone(self))
    }

    pub(super) fn get_alive_task(&self) -> i32 {
        self.alive_task.load(Ordering::Acquire)
    }
}

pub(crate) struct H2ConcurrencyTaskGuard(Arc<H2ConcurrencyStats>);

impl Drop for H2ConcurrencyTaskGuard {
    fn drop(&mut self) {
        self.0.alive_task.fetch_sub(1, Ordering::Release);
    }
}

pub(crate) struct H2ConnectionCltWrapperStats {
    server: Arc<HttpGuardServerStats>,
    site_io_stats: Arc<UserTrafficStats>,
}

impl H2ConnectionCltWrapperStats {
    pub(crate) fn new(
        server: &Arc<HttpGuardServerStats>,
        site_io_stats: Arc<UserTrafficStats>,
    ) -> Arc<Self> {
        Arc::new(H2ConnectionCltWrapperStats {
            server: Arc::clone(server),
            site_io_stats,
        })
    }
}

impl LimitedReaderStats for H2ConnectionCltWrapperStats {
    fn add_read_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_http.add_in_bytes(size);
        self.site_io_stats.io.h2_connection.add_in_bytes(size);
    }
}

impl LimitedWriterStats for H2ConnectionCltWrapperStats {
    fn add_write_bytes(&self, size: usize) {
        let size = size as u64;
        self.server.io_http.add_out_bytes(size);
        self.site_io_stats.io.h2_connection.add_out_bytes(size);
    }
}
