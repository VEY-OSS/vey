/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use core::alloc::{GlobalAlloc, Layout};

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
