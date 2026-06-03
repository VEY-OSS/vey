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
pub use remote::{UdpCopyRemoteError, UdpCopyRemoteRecv, UdpCopyRemoteSend};

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
