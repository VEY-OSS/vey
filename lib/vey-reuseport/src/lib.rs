/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use zerocopy::{Immutable, IntoBytes};

pub mod quic;
pub mod tcp;
pub mod udp;

/// Pin directory for one listener. IPv4 and IPv6 never share it: `0.0.0.0`
/// and `::` are different sockets, and a shared map collides on worker keys.
pub(crate) fn reuseport_pin_dir(protocol: &str, addr: SocketAddr) -> PathBuf {
    let ip = match addr.ip() {
        // '.' is not allowed in the pin path.
        IpAddr::V4(ip) => format!("v4-{:08x}", u32::from(ip)),
        IpAddr::V6(ip) => format!("v6-{}", ip.to_string().replace('.', "-")),
    };
    let port = addr.port();
    PathBuf::from(format!("/sys/fs/bpf/vey-reuseport/{protocol}/{ip}_{port}"))
}

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
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

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

    #[test]
    fn unspecified_v4_and_v6_do_not_share_pin_dir() {
        let v4 = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 443));
        let v6 = SocketAddr::from((Ipv6Addr::UNSPECIFIED, 443));
        let v4_dir = reuseport_pin_dir("tcp", v4);
        let v6_dir = reuseport_pin_dir("tcp", v6);

        assert_ne!(v4_dir, v6_dir);
        assert!(!v4_dir.display().to_string().contains('.'));
        assert!(!v6_dir.display().to_string().contains('.'));
        assert!(v4_dir.ends_with("v4-00000000_443"));
        assert!(v6_dir.ends_with("v6-::_443"));
    }

    #[test]
    fn ipv4_localhost_does_not_share_pin_dir_with_ipv6_localhost() {
        let v4 = SocketAddr::from((Ipv4Addr::new(0, 0, 0, 1), 443));
        let v6 = SocketAddr::from((Ipv6Addr::LOCALHOST, 443));
        assert_ne!(reuseport_pin_dir("udp", v4), reuseport_pin_dir("udp", v6));
    }

    #[test]
    fn ipv4_does_not_share_pin_dir_with_mapped_ipv6() {
        let v4 = SocketAddr::from((Ipv4Addr::LOCALHOST, 53));
        let mapped = SocketAddr::from((Ipv4Addr::LOCALHOST.to_ipv6_mapped(), 53));
        let v4_dir = reuseport_pin_dir("quic", v4);
        let v6_dir = reuseport_pin_dir("quic", mapped);

        assert_ne!(v4_dir, v6_dir);
        assert!(!v6_dir.display().to_string().contains('.'));
    }
}
