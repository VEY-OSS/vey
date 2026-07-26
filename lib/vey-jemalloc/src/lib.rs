/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_int;

pub mod stats;

mod version;
pub use version::lib_version;

pub struct Jemalloc;

const ZERO_FLAG: c_int = 0x40;

const fn align_flags(layout: Layout) -> c_int {
    layout.align().trailing_zeros() as c_int
}

unsafe impl GlobalAlloc for Jemalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { jemalloc_sys::mallocx(layout.size(), align_flags(layout)) as _ }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { jemalloc_sys::sdallocx(ptr as _, layout.size(), align_flags(layout)) as _ }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { jemalloc_sys::mallocx(layout.size(), align_flags(layout) | ZERO_FLAG) as _ }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { jemalloc_sys::rallocx(ptr as _, new_size, align_flags(layout)) as _ }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout};

    #[test]
    fn global_alloc_smoke() {
        let layout = Layout::from_size_align(128, 16).unwrap();
        let ptr = unsafe { Jemalloc.alloc(layout) };
        assert!(!ptr.is_null());
        unsafe { Jemalloc.dealloc(ptr, layout) };
    }

    #[test]
    fn global_alloc_zeroed_smoke() {
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { Jemalloc.alloc_zeroed(layout) };
        assert!(!ptr.is_null());
        let slice = unsafe { std::slice::from_raw_parts(ptr, layout.size()) };
        assert!(slice.iter().all(|b| *b == 0));
        unsafe { Jemalloc.dealloc(ptr, layout) };
    }

    #[test]
    fn global_alloc_realloc_smoke() {
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = unsafe { Jemalloc.alloc(layout) };
        assert!(!ptr.is_null());
        let new_ptr = unsafe { Jemalloc.realloc(ptr, layout, 64) };
        assert!(!new_ptr.is_null());
        unsafe { Jemalloc.dealloc(new_ptr, Layout::from_size_align(64, 8).unwrap()) };
    }

    #[test]
    fn lib_version_returns_jemalloc_version() {
        let version = lib_version().expect("jemalloc version should be available");
        let s = version.to_str().expect("version should be utf-8");
        assert!(!s.is_empty());
    }

    #[test]
    fn stats_entries_read_values() {
        let allocated = stats::allocated().expect("stats.allocated mib");
        assert_eq!(allocated.name().to_str().unwrap(), "stats.allocated");
        assert!(allocated.value().is_some());
    }
}
