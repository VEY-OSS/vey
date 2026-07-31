/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::IoSliceMut;

use bytes::{Bytes, BytesMut};

mod client;
pub use client::{
    LimitedUdpCopyClientRecv, LimitedUdpCopyClientSend, UdpCopyClientError, UdpCopyClientRecv,
    UdpCopyClientSend,
};

mod remote;
pub use remote::{
    LimitedUdpCopyRemoteRecv, LimitedUdpCopyRemoteSend, UdpCopyRemoteError, UdpCopyRemoteRecv,
    UdpCopyRemoteSend,
};

mod transfer;
pub use transfer::{UdpCopyClientToRemote, UdpCopyError, UdpCopyRemoteToClient};

pub trait AsUdpPayload {
    fn as_payload(&self) -> &[u8];
}

#[derive(Clone)]
pub struct UdpCopyPacket {
    buf: BytesMut,
    buf_data_off: usize,
    buf_data_end: usize,
}

impl UdpCopyPacket {
    pub(crate) fn new(reserved_size: usize, packet_size: u16) -> Self {
        let buf_size = packet_size as usize + reserved_size;
        UdpCopyPacket {
            buf: BytesMut::zeroed(buf_size),
            buf_data_off: 0,
            buf_data_end: 0,
        }
    }

    #[inline]
    pub fn buf_mut(&mut self) -> &mut [u8] {
        self.buf.as_mut()
    }

    #[inline]
    pub fn buf(&self) -> &[u8] {
        self.buf.as_ref()
    }

    pub fn buf_len(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn set_offset(&mut self, off: usize) {
        self.buf_data_off = off;
    }

    #[inline]
    pub fn set_length(&mut self, len: usize) {
        self.buf_data_end = len;
    }

    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[self.buf_data_off..self.buf_data_end]
    }

    pub fn payload_len(&self) -> usize {
        self.buf_data_end - self.buf_data_off
    }

    #[inline]
    pub fn into_payload(mut self) -> Bytes {
        self.buf
            .split_to(self.buf_data_end)
            .split_off(self.buf_data_off)
            .freeze()
    }
}

impl AsUdpPayload for UdpCopyPacket {
    fn as_payload(&self) -> &[u8] {
        self.payload()
    }
}

impl AsUdpPayload for Bytes {
    fn as_payload(&self) -> &[u8] {
        self.as_ref()
    }
}

pub struct UdpCopyPacketMeta {
    iov_base: *const u8,
    data_off: usize,
    data_len: usize,
}

impl UdpCopyPacketMeta {
    pub fn new(iov: &IoSliceMut, data_off: usize, data_len: usize) -> Self {
        UdpCopyPacketMeta {
            iov_base: iov.as_ptr(),
            data_off,
            data_len,
        }
    }

    pub fn set_packet(self, p: &mut UdpCopyPacket) {
        let iov_advance =
            unsafe { usize::try_from(self.iov_base.offset_from(p.buf().as_ptr())).unwrap() };
        p.set_offset(iov_advance + self.data_off);
        p.set_length(iov_advance + self.data_len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet_with_payload(reserved: usize, payload: &[u8]) -> UdpCopyPacket {
        let mut packet = UdpCopyPacket::new(reserved, 512);
        packet.buf_mut()[reserved..reserved + payload.len()].copy_from_slice(payload);
        packet.set_offset(reserved);
        packet.set_length(reserved + payload.len());
        packet
    }

    #[test]
    fn a_new_packet_reserves_room_for_the_header() {
        let packet = UdpCopyPacket::new(8, 512);
        assert_eq!(packet.buf_len(), 520);
        assert_eq!(packet.buf().len(), 520);
        assert!(packet.payload().is_empty());
        assert_eq!(packet.payload_len(), 0);
    }

    #[test]
    fn payload_covers_only_the_data_range() {
        let packet = packet_with_payload(4, b"payload");
        assert_eq!(packet.payload(), b"payload");
        assert_eq!(packet.payload_len(), 7);
        assert_eq!(packet.as_payload(), b"payload");
    }

    #[test]
    fn into_payload_drops_the_header_and_the_tail() {
        let packet = packet_with_payload(4, b"payload");
        let payload = packet.into_payload();
        assert_eq!(payload.as_ref(), b"payload");
        assert_eq!(payload.as_payload(), b"payload");
    }

    #[test]
    fn into_payload_of_an_untouched_packet_is_empty() {
        let packet = UdpCopyPacket::new(4, 512);
        assert!(packet.into_payload().is_empty());
    }

    #[test]
    fn a_cloned_packet_keeps_its_own_buffer() {
        let packet = packet_with_payload(4, b"first");
        let mut clone = packet.clone();
        clone.buf_mut()[4..9].copy_from_slice(b"other");

        assert_eq!(packet.payload(), b"first");
        assert_eq!(clone.payload(), b"other");
    }

    #[test]
    fn packet_meta_offsets_are_relative_to_the_packet_buffer() {
        let mut packet = UdpCopyPacket::new(8, 512);
        packet.buf_mut()[8..16].copy_from_slice(b"hdrHELLO");

        let meta = {
            // the iov starts 8 bytes into the packet buffer, and the payload starts
            // 3 bytes into the iov
            let (_, tail) = packet.buf_mut().split_at_mut(8);
            let iov = IoSliceMut::new(tail);
            UdpCopyPacketMeta::new(&iov, 3, 8)
        };
        meta.set_packet(&mut packet);

        assert_eq!(packet.payload(), b"HELLO");
        assert_eq!(packet.payload_len(), 5);
    }
}
