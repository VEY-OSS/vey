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

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn bpf_struct_layout_matches_expectations() {
        assert_eq!(size_of::<SocketId>(), 8);
        assert_eq!(size_of::<ProcMapKey>(), 8);
        assert_eq!(size_of::<ProcMapValue>(), 8);
        assert_eq!(size_of::<ReadOnlyData>(), 8);
    }

    #[test]
    fn socket_id_serializes_to_expected_byte_length() {
        let id = SocketId {
            pid: 4242,
            generation: 7,
            worker: 3,
        };
        let bytes = id.as_bytes();
        assert_eq!(bytes.len(), size_of::<SocketId>());
        assert_eq!(bytes.len(), 8);
    }
}
