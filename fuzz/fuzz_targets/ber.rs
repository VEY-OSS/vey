/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_codec::ber::{BerInteger, BerLength};

fuzz_target!(|data: &[u8]| {
    if let Ok(length) = BerLength::parse(data) {
        std::hint::black_box((length.value(), length.encoded_len(), length.indefinite()));
    }
    if let Ok(integer) = BerInteger::parse(data) {
        std::hint::black_box((integer.value(), integer.encoded_len()));
    }
    if let Ok(enumerated) = BerInteger::parse_enumerated_value(data) {
        std::hint::black_box((enumerated.value(), enumerated.encoded_len()));
    }
});
