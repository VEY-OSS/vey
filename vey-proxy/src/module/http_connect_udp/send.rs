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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PacketTooLarge;

pub(crate) struct HttpConnectUdpSendBuffer {
    max_packet_size: u16,
    len_encoder: VarIntEncoder,
    buffer: Vec<u8>,
    write_offset: usize,
    queued_count: usize,
}

impl HttpConnectUdpSendBuffer {
    pub(crate) fn new(max_packet_size: u16) -> Self {
        HttpConnectUdpSendBuffer {
            max_packet_size,
            len_encoder: VarIntEncoder::default(),
            buffer: Vec::new(),
            write_offset: 0,
            queued_count: 0,
        }
    }

    #[inline]
    pub(crate) fn max_packet_size(&self) -> u16 {
        self.max_packet_size
    }

    fn push_packet(&mut self, packet: &[u8]) -> Result<(), PacketTooLarge> {
        if packet.len() > self.max_packet_size as usize {
            return Err(PacketTooLarge);
        }
        self.queued_count += 1;
        self.buffer.reserve(packet.len() + 2 + 4);
        self.buffer.push(0); // Capsule Type: Datagram
        self.buffer
            .extend_from_slice(self.len_encoder.encode_u16(packet.len() as u16 + 1));
        self.buffer.push(0); // Context ID
        self.buffer.extend_from_slice(packet);
        Ok(())
    }

    /// The packets left by a poll_write that returned Pending are always at the head of
    /// the next batch, so only the newly appended tail needs to be queued.
    ///
    /// Return the count of caller packets held in this buffer.
    #[allow(unused)]
    pub(crate) fn queue_packets(
        &mut self,
        packets: &[UdpCopyPacket],
    ) -> Result<usize, PacketTooLarge> {
        debug_assert!(self.queued_count <= packets.len());
        for packet in packets.iter().skip(self.queued_count) {
            self.push_packet(packet.payload())?;
        }
        Ok(self.queued_count)
    }

    /// See [`Self::queue_packets`]
    #[allow(unused)]
    pub(crate) fn queue_many_bytes(
        &mut self,
        packets: &[bytes::Bytes],
    ) -> Result<usize, PacketTooLarge> {
        debug_assert!(self.queued_count <= packets.len());
        for packet in packets.iter().skip(self.queued_count) {
            self.push_packet(packet)?;
        }
        Ok(self.queued_count)
    }

    /// Queue the packet, which is skipped if it has been queued by a poll_write that
    /// returned Pending
    pub(crate) fn queue_packet(&mut self, packet: &[u8]) -> Result<(), PacketTooLarge> {
        debug_assert!(self.queued_count <= 1);
        if self.queued_count == 0 {
            self.push_packet(packet)?;
        }
        Ok(())
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
                self.queued_count = 0;
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::task::Waker;

    use bytes::Bytes;

    struct TestWriter {
        allowed: usize,
        written: Vec<u8>,
    }

    impl TestWriter {
        fn new(allowed: usize) -> Self {
            TestWriter {
                allowed,
                written: Vec::new(),
            }
        }
    }

    impl AsyncWrite for TestWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            if self.allowed == 0 {
                return Poll::Pending;
            }
            let nw = buf.len().min(self.allowed);
            self.written.extend_from_slice(&buf[..nw]);
            self.allowed -= nw;
            Poll::Ready(Ok(nw))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn capsule(payload: &[u8]) -> Vec<u8> {
        let mut v = vec![0, payload.len() as u8 + 1, 0];
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn test_send_buffer_format() {
        let mut send_buf = HttpConnectUdpSendBuffer::new(128);
        let payload = b"hello";
        send_buf.push_packet(payload).unwrap();

        // Expected format:
        // Capsule Type: 0 (1 byte)
        // Capsule Length: payload.len() + 1 = 6 (1 byte VarInt)
        // Context ID: 0 (1 byte)
        // Payload: "hello" (5 bytes)
        let expected = vec![0, 6, 0, b'h', b'e', b'l', b'l', b'o'];
        assert_eq!(send_buf.buffer, expected);
    }

    #[test]
    fn test_send_buffer_oversized_packet_rejected() {
        let mut send_buf = HttpConnectUdpSendBuffer::new(4);
        assert_eq!(send_buf.push_packet(b"oversized"), Err(PacketTooLarge));
        assert!(send_buf.buffer.is_empty());
        assert_eq!(send_buf.queued_count, 0);
    }

    #[test]
    fn test_queue_many_bytes_append_new_tail() {
        let mut send_buf = HttpConnectUdpSendBuffer::new(128);
        let mut cx = Context::from_waker(Waker::noop());
        let mut writer = TestWriter::new(0);

        let packets = vec![Bytes::from_static(b"aa"), Bytes::from_static(b"bb")];
        assert_eq!(send_buf.queue_many_bytes(&packets).unwrap(), 2);
        assert!(
            send_buf
                .poll_write(&mut cx, Pin::new(&mut writer))
                .is_pending()
        );

        // the caller retries with two more packets appended
        let packets = vec![
            Bytes::from_static(b"aa"),
            Bytes::from_static(b"bb"),
            Bytes::from_static(b"cc"),
            Bytes::from_static(b"dd"),
        ];
        assert_eq!(send_buf.queue_many_bytes(&packets).unwrap(), 4);

        let mut expected = Vec::new();
        for payload in [b"aa", b"bb", b"cc", b"dd"] {
            expected.extend_from_slice(&capsule(payload));
        }
        assert_eq!(send_buf.buffer, expected);

        let mut writer = TestWriter::new(expected.len());
        assert!(
            send_buf
                .poll_write(&mut cx, Pin::new(&mut writer))
                .is_ready()
        );
        assert_eq!(writer.written, expected);
        assert_eq!(send_buf.queued_count, 0);
    }

    #[test]
    fn test_queue_packet_skip_pending() {
        let mut send_buf = HttpConnectUdpSendBuffer::new(128);
        let mut cx = Context::from_waker(Waker::noop());

        let mut writer = TestWriter::new(2);
        send_buf.queue_packet(b"hello").unwrap();
        assert!(
            send_buf
                .poll_write(&mut cx, Pin::new(&mut writer))
                .is_pending()
        );

        // the same packet is retried, it should not be queued twice
        send_buf.queue_packet(b"hello").unwrap();
        assert_eq!(send_buf.buffer, capsule(b"hello"));

        let mut writer = TestWriter::new(1024);
        assert!(
            send_buf
                .poll_write(&mut cx, Pin::new(&mut writer))
                .is_ready()
        );
        assert_eq!(send_buf.queued_count, 0);
    }
}
