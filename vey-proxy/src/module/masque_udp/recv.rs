/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use thiserror::Error;
use tokio::io::{AsyncRead, ReadBuf};

use vey_codec::quic::VarInt;
use vey_io_ext::{UdpCopyClientError, UdpCopyRemoteError};

#[derive(Debug, Error)]
pub(crate) enum MasqueUdpRecvError {
    #[error("io failed: {0}")]
    IoFailed(#[from] io::Error),
    #[error("io closed")]
    IoClosed,
    #[error("invalid context id {0}")]
    InvalidContextId(u64),
    #[error("invalid capsule type {0}")]
    InvalidCapsuleType(u64),
    #[error("invalid packet size {0}")]
    InvalidPacketSize(u64),
}

impl From<MasqueUdpRecvError> for UdpCopyClientError {
    fn from(value: MasqueUdpRecvError) -> Self {
        match value {
            MasqueUdpRecvError::IoFailed(e) => UdpCopyClientError::RecvFailed(e),
            MasqueUdpRecvError::IoClosed => UdpCopyClientError::RecvClosed,
            MasqueUdpRecvError::InvalidContextId(v) => UdpCopyClientError::InvalidPacket(format!(
                "invalid context id {v} while reading masque udp capsule header"
            )),
            MasqueUdpRecvError::InvalidCapsuleType(v) => UdpCopyClientError::InvalidPacket(
                format!("invalid capsule type {v} while reading masque udp capsule header"),
            ),
            MasqueUdpRecvError::InvalidPacketSize(v) => UdpCopyClientError::InvalidPacket(format!(
                "invalid packet size {v} while reading masque udp capsule header"
            )),
        }
    }
}

impl From<MasqueUdpRecvError> for UdpCopyRemoteError {
    fn from(value: MasqueUdpRecvError) -> Self {
        match value {
            MasqueUdpRecvError::IoFailed(e) => UdpCopyRemoteError::RecvFailed(e),
            MasqueUdpRecvError::IoClosed => UdpCopyRemoteError::RecvClosed,
            MasqueUdpRecvError::InvalidContextId(v) => UdpCopyRemoteError::InvalidPacket(format!(
                "invalid context id {v} while reading masque udp capsule header"
            )),
            MasqueUdpRecvError::InvalidCapsuleType(v) => UdpCopyRemoteError::InvalidPacket(
                format!("invalid capsule type {v} while reading masque udp capsule header"),
            ),
            MasqueUdpRecvError::InvalidPacketSize(v) => UdpCopyRemoteError::InvalidPacket(format!(
                "invalid packet size {v} while reading masque udp capsule header"
            )),
        }
    }
}

#[derive(Clone, Copy)]
struct Datagram {
    length: usize,
    start: usize,
    left: usize,
}

pub(crate) struct MasqueUdpRecvBuffer {
    max_packet_size: usize,
    buffer: Box<[u8]>,
    datagram: Option<Datagram>,
    parse_start: usize,
    read_start: usize,
}

impl MasqueUdpRecvBuffer {
    pub(crate) fn new(capacity: usize, max_packet_size: u16) -> Self {
        let capacity = capacity.max(max_packet_size as usize + 24); // at least for 1 packet
        MasqueUdpRecvBuffer {
            max_packet_size: max_packet_size as usize,
            buffer: vec![0u8; capacity].into_boxed_slice(),
            datagram: None,
            parse_start: 0,
            read_start: 0,
        }
    }

    pub(crate) fn consume_datagram(&mut self) {
        let Some(datagram) = self.datagram.take() else {
            return;
        };
        if datagram.left != 0 {
            self.datagram = Some(datagram);
            return;
        }
        self.parse_start = datagram.start + datagram.length;
    }

    pub(crate) fn poll_datagram<R>(
        &mut self,
        cx: &mut Context<'_>,
        mut reader: Pin<&mut R>,
    ) -> Poll<Result<&[u8], MasqueUdpRecvError>>
    where
        R: AsyncRead + Unpin,
    {
        loop {
            if let Some(mut datagram) = self.datagram.take() {
                if datagram.left == 0 {
                    self.datagram = Some(datagram);
                    return Poll::Ready(Ok(
                        &self.buffer[datagram.start..datagram.start + datagram.length]
                    ));
                }

                let mut read_buf = ReadBuf::new(&mut self.buffer[self.read_start..]);
                match reader.as_mut().poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(())) => {
                        let nr = read_buf.filled().len();
                        if nr == 0 {
                            return Poll::Ready(Err(MasqueUdpRecvError::IoFailed(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "unexpected eof while reading datagram data",
                            ))));
                        }
                        self.read_start += nr;
                        if nr > datagram.left {
                            datagram.left = 0;
                        } else {
                            datagram.left -= nr;
                        }
                        self.datagram = Some(datagram);
                        continue;
                    }
                    Poll::Ready(Err(e)) => {
                        self.datagram = Some(datagram);
                        return Poll::Ready(Err(MasqueUdpRecvError::IoFailed(e)));
                    }
                    Poll::Pending => {
                        self.datagram = Some(datagram);
                        return Poll::Pending;
                    }
                }
            }

            if self.read_start > self.parse_start {
                self.parse_header()?;
            }

            if let Some(mut datagram) = self.datagram.take() {
                let expected_end = datagram.start + datagram.length;
                if self.read_start >= expected_end {
                    datagram.left = 0;
                    self.datagram = Some(datagram);
                    return Poll::Ready(Ok(
                        &self.buffer[datagram.start..datagram.start + datagram.length]
                    ));
                }
                datagram.left = datagram.length - (self.read_start - datagram.start);

                if self.buffer.len() < expected_end {
                    // no enough space for a full datagram packet read
                    self.buffer
                        .copy_within(self.parse_start..self.read_start, 0);
                    datagram.start -= self.parse_start;
                    self.read_start -= self.parse_start;
                    self.parse_start = 0;
                }

                self.datagram = Some(datagram);
            } else {
                if self.parse_start != 0 {
                    // move the partial header to the beginning of the buffer
                    self.buffer
                        .copy_within(self.parse_start..self.read_start, 0);
                    self.read_start -= self.parse_start;
                    self.parse_start = 0;
                }

                let mut read_buf = ReadBuf::new(&mut self.buffer[self.read_start..]);
                ready!(reader.as_mut().poll_read(cx, &mut read_buf))?;
                let nr = read_buf.filled().len();
                if nr == 0 {
                    return if self.read_start == 0 {
                        Poll::Ready(Err(MasqueUdpRecvError::IoClosed))
                    } else {
                        Poll::Ready(Err(MasqueUdpRecvError::IoFailed(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "unexpected eof while reading masque udp capsule header",
                        ))))
                    };
                }
                self.read_start += nr;
            }
        }
    }

    fn parse_header(&mut self) -> Result<(), MasqueUdpRecvError> {
        let left_data = &self.buffer[self.parse_start..self.read_start];

        // Context ID
        let mut offset = 0;
        match VarInt::parse(left_data) {
            Some(data) => {
                let context_id = data.value();
                if context_id != 0 {
                    return Err(MasqueUdpRecvError::InvalidContextId(context_id));
                }
                offset += data.encoded_len();
            }
            None => return Ok(()),
        }

        // Capsule Type
        match VarInt::parse(&left_data[offset..]) {
            Some(data) => {
                let capsule_type = data.value();
                if capsule_type != 0 {
                    return Err(MasqueUdpRecvError::InvalidCapsuleType(capsule_type));
                }
                offset += data.encoded_len();
            }
            None => return Ok(()),
        }

        // Capsule Length
        if let Some(data) = VarInt::parse(&left_data[offset..]) {
            let capsule_length = data.value();
            if capsule_length > self.max_packet_size as u64 {
                return Err(MasqueUdpRecvError::InvalidPacketSize(capsule_length));
            }
            let datagram_len = capsule_length as usize;
            offset += data.encoded_len();
            let datagram = Datagram {
                length: datagram_len,
                start: self.parse_start + offset,
                left: datagram_len,
            };
            self.datagram = Some(datagram);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::poll_fn;

    use tokio_test::io::Builder as MockIoBuilder;
    use vey_codec::quic::VarIntEncoder;

    fn capsule(payload: &[u8]) -> Vec<u8> {
        let mut encoder = VarIntEncoder::default();
        let mut buf = Vec::with_capacity(payload.len() + 6);
        buf.push(0); // Context ID
        buf.push(0); // Capsule Type: Datagram
        buf.extend_from_slice(encoder.encode_u16(payload.len() as u16));
        buf.extend_from_slice(payload);
        buf
    }

    async fn next_datagram(
        buffer: &mut MasqueUdpRecvBuffer,
        reader: &mut tokio_test::io::Mock,
    ) -> Result<Vec<u8>, MasqueUdpRecvError> {
        poll_fn(
            |cx| match buffer.poll_datagram(cx, Pin::new(&mut *reader)) {
                Poll::Ready(Ok(datagram)) => Poll::Ready(Ok(datagram.to_vec())),
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            },
        )
        .await
    }

    #[tokio::test]
    async fn decode_single_datagram() {
        let payload = b"hello";
        let data = capsule(payload);
        let mut reader = MockIoBuilder::new().read(&data).build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 128);

        let datagram = next_datagram(&mut buffer, &mut reader).await.unwrap();
        assert_eq!(datagram, payload);

        buffer.consume_datagram();
    }

    #[tokio::test]
    async fn decode_datagram_from_split_header_and_payload() {
        let payload = vec![b'x'; 70];
        let data = capsule(&payload);
        let mut reader = MockIoBuilder::new()
            .read(&data[..1])
            .read(&data[1..2])
            .read(&data[2..3])
            .read(&data[3..20])
            .read(&data[20..])
            .build();
        let mut buffer = MasqueUdpRecvBuffer::new(16, 128);

        let datagram = next_datagram(&mut buffer, &mut reader).await.unwrap();
        assert_eq!(datagram, payload);

        buffer.consume_datagram();
    }

    #[tokio::test]
    async fn decode_multiple_datagrams_from_one_read() {
        let payloads: [&[u8]; 3] = [b"one", b"two", b"three"];
        let data = payloads
            .iter()
            .flat_map(|payload| capsule(payload))
            .collect::<Vec<_>>();
        let mut reader = MockIoBuilder::new().read(&data).build();
        let mut buffer = MasqueUdpRecvBuffer::new(16, 128);

        for payload in payloads {
            let datagram = next_datagram(&mut buffer, &mut reader).await.unwrap();
            assert_eq!(datagram, payload);
            buffer.consume_datagram();
        }
    }

    #[tokio::test]
    async fn decode_after_compacting_consumed_prefix() {
        let payloads: [&[u8]; 4] = [b"aaaaaa", b"bbbbbb", b"cccccc", b"dddddd"];
        let data = payloads
            .iter()
            .flat_map(|payload| capsule(payload))
            .collect::<Vec<_>>();
        let mut reader = MockIoBuilder::new()
            .read(&data[..30])
            .read(&data[30..])
            .build();
        let mut buffer = MasqueUdpRecvBuffer::new(30, 6);

        for payload in payloads {
            let datagram = next_datagram(&mut buffer, &mut reader).await.unwrap();
            assert_eq!(datagram, payload);
            buffer.consume_datagram();
        }
    }

    #[tokio::test]
    async fn decode_empty_datagram() {
        let data = capsule(b"");
        let mut reader = MockIoBuilder::new().read(&data).build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 128);

        let datagram = next_datagram(&mut buffer, &mut reader).await.unwrap();
        assert!(datagram.is_empty());

        buffer.consume_datagram();
    }

    #[tokio::test]
    async fn reject_non_zero_context_id() {
        let data = [1, 0, 0];
        let mut reader = MockIoBuilder::new().read(&data).build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 128);

        let err = next_datagram(&mut buffer, &mut reader).await.unwrap_err();
        assert!(matches!(err, MasqueUdpRecvError::InvalidContextId(1)));
    }

    #[tokio::test]
    async fn reject_non_datagram_capsule_type() {
        let data = [0, 1, 0];
        let mut reader = MockIoBuilder::new().read(&data).build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 128);

        let err = next_datagram(&mut buffer, &mut reader).await.unwrap_err();
        assert!(matches!(err, MasqueUdpRecvError::InvalidCapsuleType(1)));
    }

    #[tokio::test]
    async fn reject_datagram_larger_than_max_packet_size() {
        let data = capsule(b"oversized");
        let mut reader = MockIoBuilder::new().read(&data).build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 4);

        let err = next_datagram(&mut buffer, &mut reader).await.unwrap_err();
        assert!(matches!(err, MasqueUdpRecvError::InvalidPacketSize(9)));
    }

    #[tokio::test]
    async fn report_closed_before_header() {
        let mut reader = MockIoBuilder::new().read(b"").build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 128);

        let err = next_datagram(&mut buffer, &mut reader).await.unwrap_err();
        assert!(matches!(err, MasqueUdpRecvError::IoClosed));
    }

    #[tokio::test]
    async fn report_unexpected_eof_in_payload() {
        let data = capsule(b"payload");
        let mut reader = MockIoBuilder::new()
            .read(&data[..data.len() - 1])
            .read(b"")
            .build();
        let mut buffer = MasqueUdpRecvBuffer::new(8, 128);

        let err = next_datagram(&mut buffer, &mut reader).await.unwrap_err();
        assert!(
            matches!(err, MasqueUdpRecvError::IoFailed(e) if e.kind() == io::ErrorKind::UnexpectedEof)
        );
    }
}
