/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use vey_types::metrics::NodeName;
use vey_types::stats::StatId;

pub(crate) struct ServerStats {
    name: NodeName,
    id: StatId,
    alive: AtomicI32,
    request_total: AtomicU64,
    request_invalid: AtomicU64,
    lookup_not_found: AtomicU64,
    response_total: AtomicU64,
    response_fail: AtomicU64,
}

impl ServerStats {
    pub(crate) fn new(name: &NodeName) -> Self {
        ServerStats {
            name: name.clone(),
            id: StatId::new_unique(),
            alive: AtomicI32::new(0),
            request_total: AtomicU64::new(0),
            request_invalid: AtomicU64::new(0),
            lookup_not_found: AtomicU64::new(0),
            response_total: AtomicU64::new(0),
            response_fail: AtomicU64::new(0),
        }
    }

    pub(crate) fn name(&self) -> &NodeName {
        &self.name
    }

    pub(crate) fn stat_id(&self) -> StatId {
        self.id
    }

    pub(super) fn enter_alive(&self) -> AliveGuard<'_> {
        self.alive.fetch_add(1, Ordering::Relaxed);
        AliveGuard(self)
    }

    pub(crate) fn alive_count(&self) -> i32 {
        self.alive.load(Ordering::Relaxed)
    }

    pub(crate) fn add_request_total(&self) {
        self.request_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn take_request_total(&self) -> u64 {
        self.request_total.swap(0, Ordering::Relaxed)
    }

    pub(crate) fn add_request_invalid(&self) {
        self.request_invalid.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn take_request_invalid(&self) -> u64 {
        self.request_invalid.swap(0, Ordering::Relaxed)
    }

    pub(crate) fn add_lookup_not_found(&self) {
        self.lookup_not_found.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn take_lookup_not_found(&self) -> u64 {
        self.lookup_not_found.swap(0, Ordering::Relaxed)
    }

    pub(crate) fn add_response_total(&self) {
        self.response_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn take_response_total(&self) -> u64 {
        self.response_total.swap(0, Ordering::Relaxed)
    }

    pub(crate) fn add_response_fail(&self) {
        self.response_fail.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn take_response_fail(&self) -> u64 {
        self.response_fail.swap(0, Ordering::Relaxed)
    }
}

pub(super) struct AliveGuard<'a>(&'a ServerStats);

impl Drop for AliveGuard<'_> {
    fn drop(&mut self) {
        self.0.alive.fetch_sub(1, Ordering::Relaxed);
    }
}
