/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use core::ffi::{CStr, c_char};
use core::ptr;

pub fn lib_version() -> Option<&'static CStr> {
    let mut p = ptr::null::<c_char>();
    let mut len = size_of_val(&p);

    unsafe {
        let ret = jemalloc_sys::mallctl(
            c"version".as_ptr(),
            ptr::from_mut(&mut p) as *mut _,
            ptr::from_mut(&mut len),
            ptr::null_mut(),
            0,
        );
        if ret != 0 {
            return None;
        }
    }

    unsafe { Some(CStr::from_ptr(p)) }
}
