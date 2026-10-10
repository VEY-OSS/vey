/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_smtp_proto::response::ResponseParser;

fuzz_target!(|data: &[u8]| {
    let mut parser = ResponseParser::default();
    for_each_line(data, |line| match parser.feed_line(line) {
        Ok(text) => {
            std::hint::black_box((text.len(), parser.code().as_u16(), parser.finished()));
            !parser.finished()
        }
        Err(_) => false,
    });
});

fn for_each_line(mut data: &[u8], mut feed: impl FnMut(&[u8]) -> bool) {
    loop {
        let (chunk, rest, last) = match data.iter().position(|b| *b == b'\n') {
            Some(i) => (&data[..i], &data[i + 1..], false),
            None => (data, &[][..], true),
        };
        let chunk = chunk.strip_suffix(b"\r").unwrap_or(chunk);
        let mut line = Vec::with_capacity(chunk.len() + 2);
        line.extend_from_slice(chunk);
        line.extend_from_slice(b"\r\n");
        if !feed(&line) || last {
            return;
        }
        data = rest;
        if data.is_empty() {
            return;
        }
    }
}
