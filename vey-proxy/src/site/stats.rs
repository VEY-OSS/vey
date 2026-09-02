/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use vey_types::metrics::NodeName;
use vey_types::stats::TcpIoStats;

pub(crate) struct SiteStats {
    site_group: NodeName,
    site_id: NodeName,
    request_total: AtomicU64,
    request_alive: AtomicI32,
    #[allow(dead_code)]
    pub(crate) io: TcpIoStats,
}

impl SiteStats {
    pub(super) fn new(site_group: &NodeName, site_id: &NodeName) -> Self {
        SiteStats {
            site_group: site_group.clone(),
            site_id: site_id.clone(),
            request_total: AtomicU64::new(0),
            request_alive: AtomicI32::new(0),
            io: TcpIoStats::default(),
        }
    }

    #[inline]
    pub(crate) fn site_group(&self) -> &NodeName {
        &self.site_group
    }

    #[inline]
    pub(crate) fn site_id(&self) -> &NodeName {
        &self.site_id
    }

    pub(crate) fn add_request(&self) {
        self.request_total.fetch_add(1, Ordering::Relaxed);
        self.request_alive.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn dec_alive(&self) {
        self.request_alive.fetch_sub(1, Ordering::Relaxed);
    }

    pub(crate) fn get_request_total(&self) -> u64 {
        self.request_total.load(Ordering::Relaxed)
    }

    pub(crate) fn get_alive_count(&self) -> i32 {
        self.request_alive.load(Ordering::Relaxed)
    }
}

impl fmt::Debug for SiteStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SiteStats")
            .field("site_group", self.site_group())
            .field("site_id", self.site_id())
            .field("request_total", &self.get_request_total())
            .field("request_alive", &self.get_alive_count())
            .finish()
    }
}
