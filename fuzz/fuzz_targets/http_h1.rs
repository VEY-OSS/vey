/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_http::{HttpChunkedLine, HttpHeaderLine, HttpMethodLine, HttpStatusLine};

fuzz_target!(|data: &[u8]| {
    if let Ok(line) = HttpMethodLine::parse(data) {
        std::hint::black_box((line.version, line.method.len(), line.uri.len()));
    }
    if let Ok(line) = HttpHeaderLine::parse(data) {
        std::hint::black_box((line.name.len(), line.value.len()));
    }
    if let Ok(line) = HttpStatusLine::parse(data) {
        std::hint::black_box((line.version, line.code, line.reason.len()));
    }
    if let Ok(line) = HttpChunkedLine::parse(data) {
        std::hint::black_box((line.chunk_size, line.extension.map(|ext| ext.len())));
    }
});
