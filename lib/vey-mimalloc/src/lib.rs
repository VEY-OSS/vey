/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use core::alloc::{GlobalAlloc, Layout};

pub mod stats;

mod version;
pub use version::lib_version;

pub struct Mimalloc;

unsafe impl GlobalAlloc for Mimalloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { mimalloc_sys::mi_aligned_alloc(layout.align(), layout.size()) as _ }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { mimalloc_sys::mi_free_size_aligned(ptr as _, layout.size(), layout.align()) as _ }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { mimalloc_sys::mi_zalloc_aligned(layout.size(), layout.align()) as _ }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe { mimalloc_sys::mi_realloc_aligned(ptr as _, new_size, layout.align()) as _ }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout};

    #[test]
    fn global_alloc_smoke() {
        let layout = Layout::from_size_align(128, 16).unwrap();
        let ptr = unsafe { Mimalloc.alloc(layout) };
        assert!(!ptr.is_null());
        unsafe { Mimalloc.dealloc(ptr, layout) };
    }

    #[test]
    fn global_alloc_zeroed_smoke() {
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { Mimalloc.alloc_zeroed(layout) };
        assert!(!ptr.is_null());
        let slice = unsafe { std::slice::from_raw_parts(ptr, layout.size()) };
        assert!(slice.iter().all(|b| *b == 0));
        unsafe { Mimalloc.dealloc(ptr, layout) };
    }

    #[test]
    fn global_alloc_realloc_smoke() {
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = unsafe { Mimalloc.alloc(layout) };
        assert!(!ptr.is_null());
        let new_ptr = unsafe { Mimalloc.realloc(ptr, layout, 64) };
        assert!(!new_ptr.is_null());
        unsafe { Mimalloc.dealloc(new_ptr, Layout::from_size_align(64, 8).unwrap()) };
    }

    #[test]
    fn lib_version_is_positive() {
        assert!(lib_version() > 0);
    }

    #[test]
    fn process_stats_available() {
        let stats = stats::get().expect("mimalloc stats should be readable");
        assert!(stats.current_pages >= 0);
        assert!(stats.current_commit >= 0);
    }
}
