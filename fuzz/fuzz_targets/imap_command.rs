/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use libfuzzer_sys::fuzz_target;
use vey_imap_proto::command::Command;

fuzz_target!(|data: &[u8]| {
    let line = with_crlf(data);
    if let Ok(command) = Command::parse_line(&line) {
        std::hint::black_box(command.tag.len());
        std::hint::black_box(command.parsed);
    }
});

fn with_crlf(data: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if data.ends_with(b"\r\n") {
        std::borrow::Cow::Borrowed(data)
    } else {
        let mut line = Vec::with_capacity(data.len() + 2);
        line.extend_from_slice(data);
        line.extend_from_slice(b"\r\n");
        std::borrow::Cow::Owned(line)
    }
}
