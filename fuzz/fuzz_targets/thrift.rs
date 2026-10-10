/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_codec::leb128::Leb128;
use vey_codec::thrift::VarInt32;

fuzz_target!(|data: &[u8]| {
    if let Ok(varint) = VarInt32::parse(data) {
        std::hint::black_box((
            varint.positive_value(),
            varint.value(),
            varint.encoded_len(),
        ));
    }
    if let Ok(leb) = Leb128::<u32>::decode(data) {
        std::hint::black_box((leb.value(), leb.encoded_len()));
    }
});
