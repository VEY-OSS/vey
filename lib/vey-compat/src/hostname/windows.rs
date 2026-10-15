/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows_sys::Win32::System::SystemInformation;

pub fn hostname() -> OsString {
    let mut size = 0;
    unsafe {
        SystemInformation::GetComputerNameExW(
            SystemInformation::ComputerNamePhysicalDnsHostname,
            std::ptr::null_mut(),
            &mut size,
        );
    }

    let mut buffer = vec![0u16; size as usize];
    let r = unsafe {
        SystemInformation::GetComputerNameExW(
            SystemInformation::ComputerNamePhysicalDnsHostname,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    // On success, size is the number of wchar units copied, excluding the terminating NUL.
    let len = size as usize;
    if r == 0 || len > buffer.len() {
        return OsString::new();
    }
    OsString::from_wide(&buffer[..len])
}
