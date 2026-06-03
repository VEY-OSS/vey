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
use vey_io_ext::UdpCopyClientError;

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
