/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::CRYPT_HASH64;
use crate::XCryptParseError;

const INVALID_DIGIT: u8 = 0xFF;
const B64_CRYPT_DIGITS: [u8; 256] = digit_table();

const fn digit_table() -> [u8; 256] {
    let mut table = [INVALID_DIGIT; 256];
    let mut i = 0;
    while i < CRYPT_HASH64.len() {
        table[CRYPT_HASH64[i] as usize] = i as u8;
        i += 1;
    }
    table
}

pub(crate) struct B64CryptDecoder {}

fn char_value(c: u8) -> Result<u32, XCryptParseError> {
    let value = B64_CRYPT_DIGITS[c as usize];
    if value == INVALID_DIGIT {
        Err(XCryptParseError::InvalidHash)
    } else {
        Ok(u32::from(value))
    }
}

fn accumulate(input: &[u8]) -> Result<u32, XCryptParseError> {
    let mut value = 0u32;
    for (i, &c) in input.iter().enumerate() {
        value |= char_value(c)? << (6 * i);
    }
    // A short final group still uses 6-bit characters. The encoder leaves the
    // unused high bits of that last character at zero.
    let significant_bits = match input.len() {
        2 => 8,
        3 => 16,
        4 => 24,
        _ => return Err(XCryptParseError::InvalidHash),
    };
    if value >> significant_bits != 0 {
        Err(XCryptParseError::InvalidHash)
    } else {
        Ok(value)
    }
}

impl B64CryptDecoder {
    pub(crate) fn decode_buf(input: &[u8], output: &mut [u8]) -> Result<(), XCryptParseError> {
        if input.len() != 4 {
            return Err(XCryptParseError::InvalidHash);
        }
        let value = accumulate(input)?;
        output[0] = ((value >> 16) & 0xFF) as u8;
        output[1] = ((value >> 8) & 0xFF) as u8;
        output[2] = (value & 0xFF) as u8;
        Ok(())
    }

    pub(crate) fn decode(input: &[u8]) -> Result<(u8, u8, u8), XCryptParseError> {
        let value = accumulate(input)?;
        let b2 = ((value >> 16) & 0xFF) as u8;
        let b1 = ((value >> 8) & 0xFF) as u8;
        let b0 = (value & 0xFF) as u8;
        Ok((b2, b1, b0))
    }
}
