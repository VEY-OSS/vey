/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::quic::{CryptoFrame, FrameConsume, FrameParseError};

use crate::tls::{ClientHello, ClientHelloParseError, HandshakeHeader, HandshakeType};

#[derive(PartialEq, Eq)]
struct Fragment {
    offset: usize,
    length: usize,
}

impl PartialOrd for Fragment {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fragment {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.offset.cmp(&other.offset) {
            Ordering::Greater => Ordering::Greater,
            Ordering::Less => Ordering::Less,
            Ordering::Equal => self.length.cmp(&other.length),
        }
    }
}

pub struct HandshakeCoalescer {
    max_message_size: u32,
    header: Option<HandshakeHeader>,
    buf: Vec<u8>,
    unfilled_offset: usize,
    expected_length: usize,
    fragment_set: BTreeSet<Fragment>,
}

impl Default for HandshakeCoalescer {
    fn default() -> Self {
        Self::new(1 << 14)
    }
}

impl HandshakeCoalescer {
    pub fn new(max_message_size: u32) -> Self {
        HandshakeCoalescer {
            max_message_size,
            header: None,
            buf: Vec::with_capacity(1024),
            unfilled_offset: 0,
            expected_length: 0,
            fragment_set: BTreeSet::new(),
        }
    }

    pub(crate) fn finished(&self) -> bool {
        self.expected_length > 0 && self.expected_length == self.unfilled_offset
    }

    fn advance_unfilled(&mut self, end: usize) {
        if end <= self.unfilled_offset {
            return;
        }
        self.unfilled_offset = end;
        self.fragment_set.retain(|fragment| {
            if fragment.offset > self.unfilled_offset {
                true
            } else {
                let fragment_end = fragment.offset + fragment.length;
                if fragment_end > self.unfilled_offset {
                    self.unfilled_offset = fragment_end;
                }
                false
            }
        });
    }

    fn capture_header(&mut self) -> Result<(), FrameParseError> {
        if self.expected_length > 0 {
            return Ok(());
        }
        let Some(header) = HandshakeHeader::try_parse(&self.buf[..self.unfilled_offset]) else {
            return Ok(());
        };
        if header.msg_length > self.max_message_size {
            // use the same size limit as TLS record
            return Err(FrameParseError::MalformedFrame(
                "too large message length for TLS handshake",
            ));
        }
        let msg_len = header.encoded_len();
        self.expected_length = msg_len;
        self.header = Some(header);
        // A CRYPTO frame may continue into the next handshake message.
        if self.unfilled_offset > msg_len {
            self.unfilled_offset = msg_len;
        }
        self.buf.resize(msg_len, 0);
        Ok(())
    }

    /// Parse this message as a ClientHello message
    pub fn parse_client_hello(&self) -> Result<Option<ClientHello<'_>>, ClientHelloParseError> {
        let Some(hdr) = &self.header else {
            return Ok(None);
        };
        if hdr.msg_type != HandshakeType::ClientHello as u8 {
            return Err(ClientHelloParseError::InvalidMessageType(hdr.msg_type));
        }
        if self.finished() {
            let ch = ClientHello::parse_msg_data(&self.buf[HandshakeHeader::SIZE..])?;
            Ok(Some(ch))
        } else {
            Ok(None)
        }
    }
}

impl FrameConsume for HandshakeCoalescer {
    fn recv_crypto(&mut self, frame: &CryptoFrame<'_>) -> Result<(), FrameParseError> {
        if self.expected_length > 0 && frame.stream_offset > self.expected_length {
            // invalid frames
            return Err(FrameParseError::MalformedFrame(
                "too big stream offset value in crypto frame",
            ));
        }

        // Keep only bytes that belong to this handshake message. A frame may
        // continue into the next message; those bytes are left with the caller.
        let take = if self.expected_length > 0 {
            frame
                .data
                .len()
                .min(self.expected_length - frame.stream_offset)
        } else {
            frame.data.len()
        };
        if take == 0 {
            if self.expected_length == 0 && frame.stream_offset > self.unfilled_offset {
                return Err(FrameParseError::OutOfOrderFrame(
                    "handshake header missing when receiving from crypto frame",
                ));
            }
            return Ok(());
        }

        let offset = frame.stream_offset;
        let data = &frame.data[..take];
        let end = offset + take;
        if offset <= self.unfilled_offset {
            if self.buf.len() > end {
                self.buf[offset..end].copy_from_slice(data);
            } else {
                self.buf.resize(offset, 0);
                self.buf.extend_from_slice(data);
            }
            self.advance_unfilled(end);
            self.capture_header()
        } else if self.expected_length == 0 {
            // drop all other frames if we don't know the real length
            Err(FrameParseError::OutOfOrderFrame(
                "handshake header missing when receiving from crypto frame",
            ))
        } else {
            self.buf[offset..end].copy_from_slice(data);
            self.fragment_set.insert(Fragment {
                offset,
                length: take,
            });
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    #[test]
    fn client_hello_consume() {
        let data = hex!(
            "010000ed0303ebf8fa56f129 39b9584a3896472ec40bb863cfd3e868
            04fe3a47f06a2b69484c000004130113 02010000c000000010000e00000b6578
            616d706c652e636f6dff01000100000a 00080006001d00170018001000070005
            04616c706e0005000501000000000033 00260024001d00209370b2c9caa47fba
            baf4559fedba753de171fa71f50f1ce1 5d43e994ec74d748002b000302030400
            0d0010000e0403050306030203080408 050806002d00020101001c0002400100
            3900320408ffffffffffffffff050480 00ffff07048000ffff08011001048000
            75300901100f088394c8f03e51570806 048000ffff"
        );

        let mut consumer = HandshakeCoalescer::default();
        let frame_full = CryptoFrame::new(0, &data);
        consumer.recv_crypto(&frame_full).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, data);

        let mut consumer = HandshakeCoalescer::default();
        let frame1 = CryptoFrame::new(0, &data[..30]);
        let frame2 = CryptoFrame::new(30, &data[30..]);
        consumer.recv_crypto(&frame1).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame2).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, data);

        let mut consumer = HandshakeCoalescer::default();
        let frame1 = CryptoFrame::new(0, &data[..2]);
        let frame2 = CryptoFrame::new(2, &data[2..30]);
        let frame3 = CryptoFrame::new(28, &data[28..]);
        consumer.recv_crypto(&frame1).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame2).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame3).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, data);

        let mut consumer = HandshakeCoalescer::default();
        let frame1 = CryptoFrame::new(0, &data[..4]);
        let frame2 = CryptoFrame::new(30, &data[30..]);
        let frame3 = CryptoFrame::new(4, &data[4..30]);
        consumer.recv_crypto(&frame1).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame2).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame3).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, data);

        let mut consumer = HandshakeCoalescer::default();
        let frame1 = CryptoFrame::new(0, &data[..2]);
        let frame2 = CryptoFrame::new(30, &data[30..]);
        let frame3 = CryptoFrame::new(2, &data[2..30]);
        let frame4 = CryptoFrame::new(30, &data[30..]);
        consumer.recv_crypto(&frame1).unwrap();
        assert!(!consumer.finished());
        assert!(consumer.recv_crypto(&frame2).is_err());
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame3).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame4).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, data);

        let mut consumer = HandshakeCoalescer::default();
        let frame1 = CryptoFrame::new(0, &data[..4]);
        let frame2 = CryptoFrame::new(30, &data[30..40]);
        let frame3 = CryptoFrame::new(40, &data[40..]);
        let frame4 = CryptoFrame::new(4, &data[4..30]);
        consumer.recv_crypto(&frame1).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame2).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame3).unwrap();
        assert!(!consumer.finished());
        consumer.recv_crypto(&frame4).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, data);

        let mut consumer = HandshakeCoalescer::default();
        let frame1 = CryptoFrame::new(0, &data[..30]);
        let frame2 = CryptoFrame::new(usize::MAX, &data[30..]);
        consumer.recv_crypto(&frame1).unwrap();
        assert!(!consumer.finished());
        assert!(consumer.recv_crypto(&frame2).is_err());
    }

    #[test]
    fn frame_extending_past_message_finishes() {
        // Handshake message is 6 bytes; the CRYPTO frame continues past it.
        let message = [0x16, 0x00, 0x00, 0x02, 0xAA, 0xBB];
        let mut frame = message.to_vec();
        frame.push(0xFF);

        let mut consumer = HandshakeCoalescer::default();
        consumer.recv_crypto(&CryptoFrame::new(0, &frame)).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, message);
    }

    #[test]
    fn frame_crossing_message_end_keeps_prefix() {
        let message = [0x16, 0x00, 0x00, 0x02, 0xAA, 0xBB];
        let mut consumer = HandshakeCoalescer::default();
        consumer
            .recv_crypto(&CryptoFrame::new(0, &message[..4]))
            .unwrap();
        assert!(!consumer.finished());

        // Starts at the unfilled offset and includes one byte of the next message.
        consumer
            .recv_crypto(&CryptoFrame::new(4, &[0xAA, 0xBB, 0xFF]))
            .unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, message);
    }

    #[test]
    fn out_of_order_frame_crossing_message_end_is_stored() {
        let message = [0x16, 0x00, 0x00, 0x02, 0xAA, 0xBB];
        let mut consumer = HandshakeCoalescer::default();
        consumer
            .recv_crypto(&CryptoFrame::new(0, &message[..4]))
            .unwrap();

        // Starts inside the message, past the contiguous prefix, and crosses the end.
        consumer
            .recv_crypto(&CryptoFrame::new(5, &[0xBB, 0xFF]))
            .unwrap();
        assert!(!consumer.finished());
        assert_eq!(consumer.buf[5], 0xBB);

        consumer.recv_crypto(&CryptoFrame::new(4, &[0xAA])).unwrap();
        assert!(consumer.finished());
        assert_eq!(consumer.buf, message);
    }
}
