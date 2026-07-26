/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_std_ext::core::{FromZigZag, ToZigZag};

use crate::leb128::{Leb128, Leb128DecodeError, Leb128Encoder};

pub struct VarInt32 {
    leb128: Leb128<u32>,
}

impl VarInt32 {
    pub fn parse(data: &[u8]) -> Result<VarInt32, Leb128DecodeError> {
        let leb128 = Leb128::decode(data)?;
        Ok(VarInt32 { leb128 })
    }

    // Get the varint value used directly in thrift protocol,
    // which is always positive and not zigzag encoded
    pub fn positive_value(&self) -> i32 {
        u32::cast_signed(self.leb128.value())
    }

    // Get the thrift integer value, which is zigzag encoded
    pub fn value(&self) -> i32 {
        let uv = self.leb128.value();
        i32::from_zig_zag(uv)
    }

    pub fn encoded_len(&self) -> usize {
        self.leb128.encoded_len()
    }
}

#[derive(Default)]
pub struct VarIntEncoder {
    leb128: Leb128Encoder,
}

impl VarIntEncoder {
    // Encode the thrift integer value, with correct zigzag encoding
    pub fn encode_i32(&mut self, v: i32) -> &[u8] {
        let uv = v.to_zig_zag();
        self.leb128.encode_u32(uv)
    }

    // Encode the positive varint used directly in thrift protocol, which will not be zigzag encoded
    pub fn encode_positive_i32(&mut self, v: i32) -> &[u8] {
        self.leb128.encode_u32(i32::cast_unsigned(v))
    }
}

#[cfg(test)]
mod tests {
    use super::{VarInt32, VarIntEncoder};

    #[test]
    fn zigzag_roundtrip() {
        let mut enc = VarIntEncoder::default();
        for v in [0, 1, -1, 2, -2, i32::MAX, i32::MIN, 12345, -67890] {
            let encoded = enc.encode_i32(v).to_vec();
            let parsed = VarInt32::parse(&encoded).unwrap();
            assert_eq!(parsed.value(), v, "zigzag value for {v}");
            assert_eq!(parsed.encoded_len(), encoded.len());
        }
    }

    #[test]
    fn positive_varint_no_zigzag() {
        let mut enc = VarIntEncoder::default();
        let encoded = enc.encode_positive_i32(150).to_vec();
        let parsed = VarInt32::parse(&encoded).unwrap();
        assert_eq!(parsed.positive_value(), 150);
        // Zigzag decode of the same bytes is a different integer.
        assert_ne!(parsed.value(), 150);

        let encoded = enc.encode_positive_i32(1).to_vec();
        assert_eq!(encoded, [0x01]);
        assert_eq!(VarInt32::parse(&encoded).unwrap().positive_value(), 1);
    }

    #[test]
    fn parse_errors() {
        assert!(VarInt32::parse(&[]).is_err());
        // Continuation bit set with no following byte
        assert!(VarInt32::parse(&[0x80]).is_err());
    }
}
