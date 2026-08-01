/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! Cloudflare Keyless wire decode for [`super::KeylessRequest`].

use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::rsa::Padding;
use tokio::io::{AsyncRead, AsyncReadExt};

use vey_codec::tlv::{T1L2BVParse, TlvParse};

use super::{KeylessAction, KeylessRequest, KeylessRequestError};
use crate::protocol::KeylessErrorResponse;

impl T1L2BVParse<'_> for KeylessRequest {
    type Error = KeylessRequestError;

    fn no_enough_data() -> Self::Error {
        KeylessRequestError::CorruptedMessage
    }

    fn parse_value(&mut self, tag: u8, v: &[u8]) -> Result<(), Self::Error> {
        match tag {
            // Cert Digest
            0x01 => {}
            // SKI
            0x04 => {
                self.ski = v.to_vec();
            }
            // OPCODE
            0x11 => {
                if v.len() != 1 {
                    return Err(KeylessRequestError::InvalidItemLength(tag));
                }
                self.opcode = v[0];
            }
            // PAYLOAD
            0x12 => {
                self.payload = v.to_vec();
            }
            // PADDING
            0x20 => {}
            _ => {}
        }
        Ok(())
    }
}

impl KeylessRequest {
    /// Capacity hint for the read buffer used with [`Self::read_cloudflare`].
    ///
    /// Cloudflare Keyless messages are typically padded to 1024 bytes, plus a
    /// small margin for the framing header.
    pub(crate) const CLOUDFLARE_READ_BUF_CAPACITY: usize = 1024 + 2;

    pub(crate) async fn read_cloudflare<R>(
        reader: &mut R,
        buf: &mut Vec<u8>,
        msg_count: usize,
    ) -> Result<Self, KeylessRequestError>
    where
        R: AsyncRead + Unpin,
    {
        const HDR_BUF_LEN: usize = 8;

        let mut hdr_buf = [0u8; HDR_BUF_LEN];
        match reader.read_exact(&mut hdr_buf).await {
            Ok(len) => {
                if len < HDR_BUF_LEN {
                    return if msg_count == 0 {
                        Err(KeylessRequestError::ClosedEarly)
                    } else {
                        Err(KeylessRequestError::InvalidMessageLength)
                    };
                }
            }
            Err(e) => {
                return if msg_count == 0 {
                    Err(KeylessRequestError::ClosedEarly)
                } else {
                    Err(KeylessRequestError::ReadFailed(e))
                };
            }
        }

        let major = hdr_buf[0];
        let minor = hdr_buf[1];
        if major != 1 || minor != 0 {
            return Err(KeylessRequestError::UnexpectedVersion(major, minor));
        }

        let len = ((hdr_buf[2] as usize) << 8) + hdr_buf[3] as usize;
        buf.clear();
        buf.resize(len, 0);
        match reader.read_exact(buf).await {
            Ok(nr) => {
                if nr < len {
                    return if msg_count == 0 {
                        Err(KeylessRequestError::ClosedEarly)
                    } else {
                        Err(KeylessRequestError::InvalidMessageLength)
                    };
                }
            }
            Err(e) => {
                return if msg_count == 0 {
                    Err(KeylessRequestError::ClosedEarly)
                } else {
                    Err(KeylessRequestError::ReadFailed(e))
                };
            }
        }

        let id = u32::from_be_bytes([hdr_buf[4], hdr_buf[5], hdr_buf[6], hdr_buf[7]]);
        let mut request = KeylessRequest::new(id);
        request.parse_tlv(buf)?;
        Ok(request)
    }

    pub(crate) fn verify_cloudflare_opcode(&mut self) -> Result<(), KeylessErrorResponse> {
        let action = match self.opcode {
            0x01 => KeylessAction::RsaDecrypt(Padding::PKCS1),
            0x02 => {
                self.check_payload_for_message_digest(
                    MessageDigest::from_nid(Nid::MD5_SHA1).unwrap(),
                )?;
                KeylessAction::RsaSign(Nid::MD5_SHA1)
            }
            0x03 => {
                self.check_payload_for_message_digest(MessageDigest::sha1())?;
                KeylessAction::RsaSign(Nid::SHA1)
            }
            0x04 => {
                self.check_payload_for_message_digest(MessageDigest::sha224())?;
                KeylessAction::RsaSign(Nid::SHA224)
            }
            0x05 => {
                self.check_payload_for_message_digest(MessageDigest::sha256())?;
                KeylessAction::RsaSign(Nid::SHA256)
            }
            0x06 => {
                self.check_payload_for_message_digest(MessageDigest::sha384())?;
                KeylessAction::RsaSign(Nid::SHA384)
            }
            0x07 => {
                self.check_payload_for_message_digest(MessageDigest::sha512())?;
                KeylessAction::RsaSign(Nid::SHA512)
            }
            0x08 => KeylessAction::RsaDecrypt(Padding::NONE),
            0x12 => {
                self.check_payload_for_message_digest(
                    MessageDigest::from_nid(Nid::MD5_SHA1).unwrap(),
                )?;
                KeylessAction::EcdsaSign(Nid::MD5_SHA1)
            }
            0x13 => {
                self.check_payload_for_message_digest(MessageDigest::sha1())?;
                KeylessAction::EcdsaSign(Nid::SHA1)
            }
            0x14 => {
                self.check_payload_for_message_digest(MessageDigest::sha224())?;
                KeylessAction::EcdsaSign(Nid::SHA224)
            }
            0x15 => {
                self.check_payload_for_message_digest(MessageDigest::sha256())?;
                KeylessAction::EcdsaSign(Nid::SHA256)
            }
            0x16 => {
                self.check_payload_for_message_digest(MessageDigest::sha384())?;
                KeylessAction::EcdsaSign(Nid::SHA384)
            }
            0x17 => {
                self.check_payload_for_message_digest(MessageDigest::sha512())?;
                KeylessAction::EcdsaSign(Nid::SHA512)
            }
            0x18 => KeylessAction::Ed25519Sign,
            0x35 => {
                self.check_payload_for_message_digest(MessageDigest::sha256())?;
                KeylessAction::RsaPssSign(Nid::SHA256)
            }
            0x36 => {
                self.check_payload_for_message_digest(MessageDigest::sha384())?;
                KeylessAction::RsaPssSign(Nid::SHA384)
            }
            0x37 => {
                self.check_payload_for_message_digest(MessageDigest::sha512())?;
                KeylessAction::RsaPssSign(Nid::SHA512)
            }
            0xF1 => KeylessAction::Ping,
            _ => return Err(KeylessErrorResponse::new(self.id).bad_op_code()),
        };
        self.action = action;
        Ok(())
    }

    fn check_payload_for_message_digest(
        &self,
        d: MessageDigest,
    ) -> Result<(), KeylessErrorResponse> {
        if d.size() != self.payload.len() {
            return Err(KeylessErrorResponse::new(self.id).format_error());
        }
        Ok(())
    }
}
