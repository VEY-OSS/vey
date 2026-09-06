/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use vey_daemon::stat::task::TcpStreamConnectionStats;

use crate::module::http_forward::HttpForwardTaskRemoteStats;

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

#[derive(Default)]
pub(crate) struct H2ForwardTaskStats {
    pub(crate) clt: TcpStreamConnectionStats,
    pub(crate) ups: TcpStreamConnectionStats,
}

impl HttpForwardTaskRemoteStats for H2ForwardTaskStats {
    fn add_read_bytes(&self, size: u64) {
        self.ups.read.add_bytes(size);
    }

    fn add_write_bytes(&self, size: u64) {
        self.ups.write.add_bytes(size);
    }
}
