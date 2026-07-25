/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use thiserror::Error;

use super::{BerLength, BerLengthParseError};

#[derive(Debug, PartialEq, Eq, Error)]
pub enum BerIntegerParseError {
    #[error("need {0} bytes more data")]
    NeedMoreData(usize),
    #[error("invalid ber type")]
    InvalidType,
    #[error("invalid ber length")]
    TooLargeLength,
    #[error("indefinite length")]
    IndefiniteLength,
    #[error("invalid value bytes")]
    InvalidValueBytes,
}

impl From<BerLengthParseError> for BerIntegerParseError {
    fn from(value: BerLengthParseError) -> Self {
        match value {
            BerLengthParseError::NeedMoreData(n) => BerIntegerParseError::NeedMoreData(n),
            BerLengthParseError::TooLargeValue => BerIntegerParseError::TooLargeLength,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BerInteger {
    value: i64,
    encoded_len: usize,
}

impl BerInteger {
    pub fn parse(data: &[u8]) -> Result<BerInteger, BerIntegerParseError> {
        Self::parse_with_identifier(data, 0x02)
    }

    pub fn parse_enumerated_value(data: &[u8]) -> Result<BerInteger, BerIntegerParseError> {
        Self::parse_with_identifier(data, 0x0a)
    }

    fn parse_with_identifier(data: &[u8], identifier: u8) -> Result<Self, BerIntegerParseError> {
        if data.is_empty() {
            return Err(BerIntegerParseError::NeedMoreData(1));
        }
        if data[0] != identifier {
            return Err(BerIntegerParseError::InvalidType);
        }

        let length = BerLength::parse(&data[1..])?;
        if length.indefinite() {
            return Err(BerIntegerParseError::IndefiniteLength);
        }

        let offset = 1 + length.encoded_len();
        let left = &data[offset..];
        let content_len = length.value();
        // BER/DER INTEGER is two's complement; we only accept up to 8 content bytes (i64).
        if content_len == 0 || content_len > 8 {
            return Err(BerIntegerParseError::InvalidValueBytes);
        }
        let content_len = content_len as usize;
        if left.len() < content_len {
            return Err(BerIntegerParseError::NeedMoreData(content_len - left.len()));
        }

        Ok(BerInteger {
            value: parse_twos_complement_i64(&left[..content_len]),
            encoded_len: offset + content_len,
        })
    }

    #[inline]
    pub fn encoded_len(&self) -> usize {
        self.encoded_len
    }

    #[inline]
    pub fn value(&self) -> i64 {
        self.value
    }
}

/// Decode a BER/DER INTEGER content octet string as two's complement into `i64`.
fn parse_twos_complement_i64(bytes: &[u8]) -> i64 {
    debug_assert!(!bytes.is_empty() && bytes.len() <= 8);
    let mut buf = [0u8; 8];
    let start = 8 - bytes.len();
    if bytes[0] & 0x80 != 0 {
        buf[..start].fill(0xFF);
    }
    buf[start..].copy_from_slice(bytes);
    i64::from_be_bytes(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let e = BerInteger::parse(&[0x02]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(1));

        let e = BerInteger::parse(&[0x03, 0x01, 0x02]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::InvalidType);
        let e = BerInteger::parse(&[0x02, 0x00, 0x02]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::InvalidValueBytes);

        let v = BerInteger::parse(&[0x02, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 2);
        assert_eq!(v.encoded_len(), 3);
        let v = BerInteger::parse(&[0x02, 0x81, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 2);
        assert_eq!(v.encoded_len(), 4);

        let v = BerInteger::parse(&[0x02, 0x02, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x0102);
        assert_eq!(v.encoded_len(), 4);
        let e = BerInteger::parse(&[0x02, 0x02, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(1));

        let v = BerInteger::parse(&[0x02, 0x03, 0x01, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x010102);
        assert_eq!(v.encoded_len(), 5);
        let e = BerInteger::parse(&[0x02, 0x03, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(2));

        let v = BerInteger::parse(&[0x02, 0x04, 0x01, 0x01, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x01010102);
        assert_eq!(v.encoded_len(), 6);
        let e = BerInteger::parse(&[0x02, 0x04, 0x01, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(2));

        let v = BerInteger::parse(&[0x02, 0x05, 0x01, 0x01, 0x01, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x0101010102);
        assert_eq!(v.encoded_len(), 7);
        let e = BerInteger::parse(&[0x02, 0x05, 0x01, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(3));

        let v = BerInteger::parse(&[0x02, 0x06, 0, 0x01, 0x01, 0x01, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x0101010102);
        assert_eq!(v.encoded_len(), 8);
        let e = BerInteger::parse(&[0x02, 0x06, 0x01, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(4));

        let v = BerInteger::parse(&[0x02, 0x07, 0, 0, 0x01, 0x01, 0x01, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x0101010102);
        assert_eq!(v.encoded_len(), 9);
        let e = BerInteger::parse(&[0x02, 0x07, 0x01, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(5));

        let v = BerInteger::parse(&[0x02, 0x08, 0, 0, 0, 0x01, 0x01, 0x01, 0x01, 0x02]).unwrap();
        assert_eq!(v.value, 0x0101010102);
        assert_eq!(v.encoded_len(), 10);
        let e = BerInteger::parse(&[0x02, 0x08, 0x01, 0x01]).unwrap_err();
        assert_eq!(e, BerIntegerParseError::NeedMoreData(6));
    }

    #[test]
    fn parse_twos_complement_negatives() {
        // Single-byte two's complement (X.690)
        let v = BerInteger::parse(&[0x02, 0x01, 0xFF]).unwrap();
        assert_eq!(v.value(), -1);
        let v = BerInteger::parse(&[0x02, 0x01, 0xFE]).unwrap();
        assert_eq!(v.value(), -2);
        let v = BerInteger::parse(&[0x02, 0x01, 0x80]).unwrap();
        assert_eq!(v.value(), -128);
        let v = BerInteger::parse(&[0x02, 0x01, 0x82]).unwrap();
        assert_eq!(v.value(), -126);

        // Multi-byte two's complement encodings of the same magnitudes previously
        // mis-decoded via sign-magnitude.
        let v = BerInteger::parse(&[0x02, 0x02, 0xFE, 0xFE]).unwrap();
        assert_eq!(v.value(), -0x0102);
        let v = BerInteger::parse(&[0x02, 0x03, 0xFE, 0xFE, 0xFE]).unwrap();
        assert_eq!(v.value(), -0x010102);
        let v = BerInteger::parse(&[0x02, 0x04, 0xFE, 0xFE, 0xFE, 0xFE]).unwrap();
        assert_eq!(v.value(), -0x01010102i32 as i64);

        // Sign-extend across lengths > 4
        let v = BerInteger::parse(&[0x02, 0x05, 0xFF, 0xFE, 0xFE, 0xFE, 0xFE]).unwrap();
        assert_eq!(v.value(), -0x01010102i32 as i64);
        let v = BerInteger::parse(&[0x02, 0x08, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF])
            .unwrap();
        assert_eq!(v.value(), -1);
        let v = BerInteger::parse(&[0x02, 0x08, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])
            .unwrap();
        assert_eq!(v.value(), i64::MIN);
    }
}
