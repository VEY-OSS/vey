/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::os::unix::io::AsRawFd;

use libc::c_int;

// Not yet in the crates.io libc NetBSD bindings (added in NetBSD -current 2021).
const IP_BINDANY: c_int = 27;
const IPV6_BINDANY: c_int = 64;

pub(crate) fn set_ip_bindany_v4<T: AsRawFd>(fd: &T, enable: bool) -> io::Result<()> {
    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::IPPROTO_IP,
            IP_BINDANY,
            enable as c_int,
        )?;
        Ok(())
    }
}

pub(crate) fn set_ip_bindany_v6<T: AsRawFd>(fd: &T, enable: bool) -> io::Result<()> {
    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::IPPROTO_IPV6,
            IPV6_BINDANY,
            enable as c_int,
        )?;
        Ok(())
    }
}
