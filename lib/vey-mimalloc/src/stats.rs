/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use mimalloc_sys::mi_stats_s;

pub struct MiMallocProcessStats {
    /// Current count of mimalloc pages.
    pub current_pages: i64,
    /// Peak count of mimalloc pages.
    pub peak_pages: i64,
    /// Current committed memory (backed by the page file).
    pub current_commit: i64,
    /// Peak committed memory (backed by the page file).
    pub peak_commit: i64,
}

pub fn get() -> Option<MiMallocProcessStats> {
    let mut stats: mi_stats_s = unsafe { std::mem::zeroed() };
    stats.size = size_of::<mi_stats_s>();
    stats.version = mimalloc_sys::MI_STAT_VERSION as _;

    let success = unsafe { mimalloc_sys::mi_stats_get(std::ptr::from_mut(&mut stats)) };
    if success {
        Some(MiMallocProcessStats {
            current_pages: stats.pages.current,
            peak_pages: stats.pages.peak,
            current_commit: stats.committed.current,
            peak_commit: stats.committed.peak,
        })
    } else {
        None
    }
}
