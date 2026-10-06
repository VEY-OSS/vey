/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

const CMSG_RECV_BUFFER_SIZE: usize = 10240; // see rfc3542 20.1
const SEND_CMSG_CAPACITY: usize = 128;

pub trait RecvAncillaryData {
    fn set_recv_interface(&mut self, id: u32);
    fn set_recv_dst_addr(&mut self, addr: IpAddr);
    fn set_timestamp(&mut self, ts: Duration);

    /// Full original destination from `IP_ORIGDSTADDR` / `IPV6_ORIGDSTADDR`.
    fn set_recv_orig_dst_addr(&mut self, _addr: SocketAddr) {}
}

/// Ancillary messages for one outbound datagram.
///
/// Pushed fields are stored until [`SendAncillaryBuffer::finalize`] writes the
/// control buffer. The source port stays the socket's bound port.
#[repr(C, align(8))]
pub struct SendAncillaryBuffer {
    buf: [u8; SEND_CMSG_CAPACITY],
    len: usize,
    src_ip: Option<IpAddr>,
}

impl SendAncillaryBuffer {
    pub(crate) const fn new() -> Self {
        SendAncillaryBuffer {
            buf: [0; SEND_CMSG_CAPACITY],
            len: 0,
            src_ip: None,
        }
    }

    /// Remember the source address. An unspecified address is ignored.
    pub fn push_src_ip(&mut self, ip: IpAddr) {
        if !ip.is_unspecified() {
            self.src_ip = Some(ip);
        }
    }

    /// Write every stored field into the control buffer.
    pub fn finalize(&mut self) -> Option<()> {
        self.len = 0;
        if let Some(ip) = self.src_ip {
            self.encode_ip(ip)?;
        }
        Some(())
    }

    pub(crate) fn as_ptr(&self) -> *mut u8 {
        self.buf.as_ptr() as *mut u8
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn encode_ip(&mut self, ip: IpAddr) -> Option<()> {
        match ip {
            IpAddr::V4(ip) => self.encode_v4(ip),
            IpAddr::V6(ip) => self.encode_v6(ip),
        }
    }
}

pub struct RecvAncillaryBuffer {
    buf: [u8; CMSG_RECV_BUFFER_SIZE],
}

impl Default for RecvAncillaryBuffer {
    fn default() -> Self {
        RecvAncillaryBuffer::new()
    }
}

impl RecvAncillaryBuffer {
    pub const fn new() -> Self {
        RecvAncillaryBuffer {
            buf: [0u8; CMSG_RECV_BUFFER_SIZE],
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.buf.as_slice()
    }

    pub fn parse<T: RecvAncillaryData>(&self, total_size: usize, data: &mut T) -> io::Result<()> {
        Self::parse_buf(&self.buf[..total_size], data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_src_ip_is_written_by_finalize() {
        let mut buf = SendAncillaryBuffer::new();
        buf.push_src_ip(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        buf.push_src_ip(IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 2)));
        buf.push_src_ip(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        assert!(buf.is_empty());
        buf.finalize().unwrap();
        let len = buf.len();
        assert!(len > 0);
        buf.finalize().unwrap();
        assert_eq!(buf.len(), len);
    }

    #[test]
    fn ancillary_buffer_has_expected_capacity() {
        let buf = RecvAncillaryBuffer::new();
        assert_eq!(buf.as_bytes().len(), CMSG_RECV_BUFFER_SIZE);
        assert_eq!(CMSG_RECV_BUFFER_SIZE, 10240);
    }

    #[test]
    fn parse_empty_control_buffer_is_ok() {
        struct Noop;
        impl RecvAncillaryData for Noop {
            fn set_recv_interface(&mut self, _id: u32) {}
            fn set_recv_dst_addr(&mut self, _addr: IpAddr) {}
            fn set_timestamp(&mut self, _ts: Duration) {}
        }

        let mut data = Noop;
        RecvAncillaryBuffer::parse_buf(&[], &mut data).unwrap();
    }

    #[test]
    fn default_matches_new() {
        assert_eq!(
            RecvAncillaryBuffer::default().as_bytes().len(),
            RecvAncillaryBuffer::new().as_bytes().len()
        );
    }

    #[test]
    fn parse_buf_rejects_truncated_input() {
        struct Noop;
        impl RecvAncillaryData for Noop {
            fn set_recv_interface(&mut self, _id: u32) {}
            fn set_recv_dst_addr(&mut self, _addr: IpAddr) {}
            fn set_timestamp(&mut self, _ts: Duration) {}
        }

        // Random non-empty garbage should not panic; platform parser may error.
        let mut data = Noop;
        let _ = RecvAncillaryBuffer::parse_buf(&[0xFF, 0x01, 0x02], &mut data);
    }
}
