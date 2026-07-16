/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

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
    load_generation: u32,
}
