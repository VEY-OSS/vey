/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::os::unix::io::AsRawFd;

use libc::c_int;

pub(crate) fn set_tcp_reuseport_lb_numa_current_domain<T: AsRawFd>(fd: &T) -> io::Result<()> {
    const TCP_REUSPORT_LB_NUMA_CURDOM: i32 = -1;

    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::IPPROTO_TCP,
            libc::TCP_REUSPORT_LB_NUMA,
            TCP_REUSPORT_LB_NUMA_CURDOM,
        )?;
        Ok(())
    }
}

pub(crate) fn set_ip_bindany_v4<T: AsRawFd>(fd: &T, enable: bool) -> io::Result<()> {
    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::IPPROTO_IP,
            libc::IP_BINDANY,
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
            libc::IPV6_BINDANY,
            enable as c_int,
        )?;
        Ok(())
    }
}

pub(crate) fn set_user_cookie<T: AsRawFd>(fd: &T, cookie: u32) -> io::Result<()> {
    unsafe {
        super::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_USER_COOKIE,
            cookie as c_int,
        )?;
        Ok(())
    }
}
