/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::os::fd::RawFd;

use zerocopy::{Immutable, IntoBytes};

pub mod quic;
pub mod tcp;
pub mod udp;

#[derive(IntoBytes, Immutable)]
#[repr(C)]
struct SocketId {
    pid: i32,
    generation: u16,
    worker: u16,
}

#[derive(IntoBytes, Immutable)]
#[repr(C)]
struct ProcMapKey {
    pid: i32,
    generation: u16,
    padding: u16,
}

#[derive(IntoBytes, Immutable)]
#[repr(C)]
struct ProcMapValue {
    invalid: u32,
    count: u16,
    padding: u16,
}

#[repr(C)]
struct ReadOnlyData {
    load_pid: i32,
    load_generation: u16,
}

/// Attach a BPF program to a reuseport socket via `setsockopt`.
fn attach_reuseport_ebpf(socket_fd: RawFd, prog_fd: RawFd) -> io::Result<()> {
    let ret = unsafe {
        libc::setsockopt(
            socket_fd,
            libc::SOL_SOCKET,
            libc::SO_ATTACH_REUSEPORT_EBPF,
            &prog_fd as *const RawFd as *const libc::c_void,
            size_of::<RawFd>() as libc::socklen_t,
        )
    };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
