/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::os::unix::io::AsRawFd;

use libc::c_int;

pub(crate) fn set_bindany<T: AsRawFd>(fd: &T, enable: bool) -> io::Result<()> {
    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_BINDANY,
            enable as c_int,
        )?;
        Ok(())
    }
}

pub(crate) fn set_rtable<T: AsRawFd>(fd: &T, rtable: u32) -> io::Result<()> {
    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RTABLE,
            rtable as c_int,
        )?;
        Ok(())
    }
}
