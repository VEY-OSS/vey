/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::AsyncWrite;

use vey_codec::quic::VarIntEncoder;
use vey_io_ext::UdpCopyPacket;

pub(crate) struct MasqueUdpSendBuffer {
    max_packet_size: u16,
    len_encoder: VarIntEncoder,
    buffer: Vec<u8>,
    write_offset: usize,
}

impl MasqueUdpSendBuffer {
    pub(crate) fn new(max_packet_size: u16) -> Self {
        MasqueUdpSendBuffer {
            max_packet_size,
            len_encoder: VarIntEncoder::default(),
            buffer: Vec::new(),
            write_offset: 0,
        }
    }

    pub(crate) fn push_packet(&mut self, packet: &[u8]) {
        if packet.len() > self.max_packet_size as usize {
            return;
        }
        self.buffer.reserve(packet.len() + 2 + 4);
        self.buffer.push(0); // Context ID
        self.buffer.push(0); // Capsule Type: Datagram
        self.buffer
            .extend_from_slice(self.len_encoder.encode_u16(packet.len() as u16));
        self.buffer.extend_from_slice(packet);
    }

    pub(crate) fn push_or_drop_packet(&mut self, packet: &[u8]) {
        if !self.buffer.is_empty() {
            return;
        }
        self.push_packet(packet);
    }

    #[allow(unused)]
    pub(crate) fn push_or_drop_packets(&mut self, packets: &[UdpCopyPacket]) {
        if !self.buffer.is_empty() {
            return;
        }
        for packet in packets {
            self.push_packet(packet.buf());
        }
    }

    #[allow(unused)]
    pub(crate) fn push_or_drop_many_bytes(&mut self, packets: &[bytes::Bytes]) {
        if !self.buffer.is_empty() {
            return;
        }
        for packet in packets {
            self.push_packet(packet);
        }
    }

    pub(crate) fn poll_write<W>(
        &mut self,
        cx: &mut Context<'_>,
        mut writer: Pin<&mut W>,
    ) -> Poll<io::Result<()>>
    where
        W: AsyncWrite + Unpin,
    {
        loop {
            if self.write_offset >= self.buffer.len() {
                self.write_offset = 0;
                self.buffer.clear();
                return Poll::Ready(Ok(()));
            }
            let nw = ready!(
                writer
                    .as_mut()
                    .poll_write(cx, &self.buffer[self.write_offset..])
            )?;
            self.write_offset += nw;
        }
    }
}
