/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};

use vey_io_ext::haproxy::ProxyProtocolReadError;
use vey_types::metrics::NodeName;
use vey_types::stats::StatId;

#[derive(Default)]
pub struct ListenSnapshot {
    pub accepted: u64,
    pub dropped: u64,
    pub timeout: u64,
    pub failed: u64,
    pub datagram_recv_total: u64,
    pub datagram_recv_dropped: u64,
    pub datagram_recv_ok: u64,
    pub datagram_send_queued: u64,
    pub datagram_send_failed: u64,
    pub datagram_send_ok: u64,
}

#[derive(Debug)]
pub struct ListenStats {
    name: NodeName,
    id: StatId,

    runtime_count: AtomicIsize,
    accepted: AtomicU64,
    dropped: AtomicU64,
    timeout: AtomicU64,
    failed: AtomicU64,
    datagram_recv_total: AtomicU64,
    datagram_recv_dropped: AtomicU64,
    datagram_recv_ok: AtomicU64,
    datagram_send_queued: AtomicU64,
    datagram_send_failed: AtomicU64,
    datagram_send_ok: AtomicU64,
}

impl ListenStats {
    pub fn new(name: &NodeName) -> Self {
        ListenStats {
            name: name.clone(),
            id: StatId::new_unique(),
            runtime_count: AtomicIsize::new(0),
            accepted: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            timeout: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            datagram_recv_total: AtomicU64::new(0),
            datagram_recv_dropped: AtomicU64::new(0),
            datagram_recv_ok: AtomicU64::new(0),
            datagram_send_queued: AtomicU64::new(0),
            datagram_send_failed: AtomicU64::new(0),
            datagram_send_ok: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn name(&self) -> &NodeName {
        &self.name
    }

    #[inline]
    pub fn stat_id(&self) -> StatId {
        self.id
    }

    #[must_use]
    pub fn add_running_runtime(self: &Arc<Self>) -> ListenAliveGuard {
        self.runtime_count.fetch_add(1, Ordering::Relaxed);
        ListenAliveGuard(self.clone())
    }

    pub fn running_runtime_count(&self) -> isize {
        self.runtime_count.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn is_running(&self) -> bool {
        self.running_runtime_count() > 0
    }

    pub fn add_accepted(&self) {
        self.accepted.fetch_add(1, Ordering::Relaxed);
    }
    pub fn accepted(&self) -> u64 {
        self.accepted.load(Ordering::Relaxed)
    }

    pub fn add_dropped(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    pub fn add_timeout(&self) {
        self.timeout.fetch_add(1, Ordering::Relaxed);
    }
    pub fn timeout(&self) -> u64 {
        self.timeout.load(Ordering::Relaxed)
    }

    pub fn add_failed(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
    }
    pub fn failed(&self) -> u64 {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn add_datagram_recv_total(&self, n: u64) {
        self.datagram_recv_total.fetch_add(n, Ordering::Relaxed);
    }

    pub fn datagram_recv_total(&self) -> u64 {
        self.datagram_recv_total.load(Ordering::Relaxed)
    }

    pub fn add_datagram_recv_dropped(&self, n: u64) {
        self.datagram_recv_dropped.fetch_add(n, Ordering::Relaxed);
    }

    pub fn datagram_recv_dropped(&self) -> u64 {
        self.datagram_recv_dropped.load(Ordering::Relaxed)
    }

    pub fn add_datagram_recv_ok(&self, n: u64) {
        self.datagram_recv_ok.fetch_add(n, Ordering::Relaxed);
    }

    pub fn datagram_recv_ok(&self) -> u64 {
        self.datagram_recv_ok.load(Ordering::Relaxed)
    }

    pub fn add_datagram_send_queued(&self, n: u64) {
        self.datagram_send_queued.fetch_add(n, Ordering::Relaxed);
    }

    pub fn datagram_send_queued(&self) -> u64 {
        self.datagram_send_queued.load(Ordering::Relaxed)
    }

    pub fn add_datagram_send_failed(&self, n: u64) {
        self.datagram_send_failed.fetch_add(n, Ordering::Relaxed);
    }

    pub fn datagram_send_failed(&self) -> u64 {
        self.datagram_send_failed.load(Ordering::Relaxed)
    }

    pub fn add_datagram_send_ok(&self, n: u64) {
        self.datagram_send_ok.fetch_add(n, Ordering::Relaxed);
    }

    pub fn datagram_send_ok(&self) -> u64 {
        self.datagram_send_ok.load(Ordering::Relaxed)
    }

    pub fn add_by_proxy_protocol_error(&self, e: ProxyProtocolReadError) {
        match e {
            ProxyProtocolReadError::ReadTimeout => self.add_timeout(),
            ProxyProtocolReadError::ReadFailed(_) | ProxyProtocolReadError::ClosedUnexpected => {
                self.add_failed()
            }
            ProxyProtocolReadError::InvalidMagicHeader
            | ProxyProtocolReadError::InvalidDataLength(_)
            | ProxyProtocolReadError::InvalidVersion(_)
            | ProxyProtocolReadError::InvalidCommand(_)
            | ProxyProtocolReadError::InvalidFamily(_)
            | ProxyProtocolReadError::InvalidProtocol(_)
            | ProxyProtocolReadError::InvalidSrcAddr
            | ProxyProtocolReadError::InvalidDstAddr => self.add_dropped(),
        }
    }
}

pub struct ListenAliveGuard(Arc<ListenStats>);

impl Drop for ListenAliveGuard {
    fn drop(&mut self) {
        self.0.runtime_count.fetch_sub(1, Ordering::Relaxed);
    }
}
