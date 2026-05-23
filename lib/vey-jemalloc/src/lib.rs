/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_int;

pub mod stats;

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
