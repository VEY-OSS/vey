/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[derive(Debug)]
pub struct VarInt {
    value: u64,
    encoded_len: usize,
}

impl VarInt {
    /// Try to parse a QUIC variant-length int value from the buffer
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        let value0 = data[0] & 0b0011_1111;
        match data[0] >> 6 {
            0 => Some(VarInt {
                value: value0 as u64,
                encoded_len: 1,
            }),
            1 => {
                if data.len() < 2 {
                    return None;
                }
                Some(VarInt {
                    value: u16::from_be_bytes([value0, data[1]]) as u64,
                    encoded_len: 2,
                })
            }
            2 => {
                if data.len() < 4 {
                    return None;
                }
                Some(VarInt {
                    value: u32::from_be_bytes([value0, data[1], data[2], data[3]]) as u64,
                    encoded_len: 4,
                })
            }
            3 => {
                if data.len() < 8 {
                    return None;
                }
                Some(VarInt {
                    value: u64::from_be_bytes([
                        value0, data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                    ]),
                    encoded_len: 8,
                })
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    #[inline]
    pub fn value(&self) -> u64 {
        self.value
    }
}

#[derive(Clone, Copy, Default)]
pub struct VarIntEncoder {
    buf: [u8; 8],
}

impl VarIntEncoder {
    pub fn encode_u16(&mut self, value: u16) -> &[u8] {
        let high_byte = (value >> 8) as u8;
        let low_byte = (value & 0xff) as u8;
        if high_byte & 0b1100_0000 != 0 {
            self.buf[0] = 0b1000_0000;
            self.buf[1] = 0;
            self.buf[2] = high_byte;
            self.buf[3] = low_byte;
            &self.buf[..4]
        } else if high_byte != 0 || (low_byte & 0b1100_0000) != 0 {
            self.buf[0] = high_byte | 0b0100_0000;
            self.buf[1] = low_byte;
            &self.buf[..2]
        } else {
            self.buf[0] = low_byte;
            &self.buf[..1]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        assert!(VarInt::parse(b"").is_none());

        let v = VarInt::parse(&[0x02]).unwrap();
        assert_eq!(v.value, 2);
        assert_eq!(v.encoded_len(), 1);

        assert!(VarInt::parse(&[0b0100_1111]).is_none());
        let v = VarInt::parse(&[0b0100_1111, 0]).unwrap();
        assert_eq!(v.value, 0x0F00);
        assert_eq!(v.encoded_len(), 2);

        assert!(VarInt::parse(&[0b1000_1111, 0x00]).is_none());
        let v = VarInt::parse(&[0b1000_1111, 0, 0, 0x01]).unwrap();
        assert_eq!(v.value, 0x0F000001);
        assert_eq!(v.encoded_len(), 4);

        assert!(VarInt::parse(&[0b1100_1111, 0]).is_none());
        let v = VarInt::parse(&[0b1100_1111, 0, 0, 0, 0, 0, 0, 0x01]).unwrap();
        assert_eq!(v.value, 0x0F00000000000001);
        assert_eq!(v.encoded_len(), 8);
    }

    #[test]
    fn encode_u16() {
        let mut encoder = VarIntEncoder::default();

        let buf = &[0b1000_0000, 0, 0xFF, 0xFF];
        assert_eq!(encoder.encode_u16(u16::MAX), buf);
        let v = VarInt::parse(buf).unwrap();
        assert_eq!(v.encoded_len(), 4);
        assert_eq!(v.value(), u16::MAX as u64);

        let buf = &[0b0111_1111, 0xFF];
        assert_eq!(encoder.encode_u16(0x3FFF), buf);
        let v = VarInt::parse(buf).unwrap();
        assert_eq!(v.encoded_len(), 2);
        assert_eq!(v.value(), 0x3FFF);

        let buf = &[0b0011_1111];
        assert_eq!(encoder.encode_u16(0x3F), buf);
        let v = VarInt::parse(buf).unwrap();
        assert_eq!(v.encoded_len(), 1);
        assert_eq!(v.value(), 0x3F);

        assert_eq!(encoder.encode_u16(0), &[0]);
    }
}
