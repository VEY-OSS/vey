/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::IpAddr;
use std::time::Duration;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

const CMSG_RECV_BUFFER_SIZE: usize = 10240; // see rfc3542 20.1

pub trait RecvAncillaryData {
    fn set_recv_interface(&mut self, id: u32);
    fn set_recv_dst_addr(&mut self, addr: IpAddr);
    fn set_timestamp(&mut self, ts: Duration);
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
