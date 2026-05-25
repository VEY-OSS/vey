/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

pub fn lib_version() -> i32 {
    unsafe { mimalloc_sys::mi_version() }
}
