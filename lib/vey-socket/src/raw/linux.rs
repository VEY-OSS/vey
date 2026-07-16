/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::os::fd::AsRawFd;

use crate::RawSocket;

impl RawSocket {
    pub fn as_ebpf_fd(&self) -> u64 {
        self.get_inner().as_raw_fd() as u64
    }

    pub fn attach_reuseport_ebpf<T: AsRawFd>(&self, prog_fd: &T) -> io::Result<()> {
        crate::sockopt::attach_reuseport_ebpf(self.get_inner(), prog_fd)
    }

    pub fn so_cookie(&self) -> io::Result<u64> {
        crate::sockopt::get_so_cookie(self.get_inner())
    }
}
