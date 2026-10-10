/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![no_main]

use std::borrow::Cow;

use libfuzzer_sys::fuzz_target;
use vey_imap_proto::response::{Response, UntaggedResponse};

fuzz_target!(|data: &[u8]| {
    let line = with_crlf(data);
    let Ok(response) = Response::parse_line(&line) else {
        return;
    };
    match response {
        Response::CommandResult(tagged) => {
            std::hint::black_box((tagged.tag.len(), tagged.result));
        }
        Response::ServerStatus(status) => {
            std::hint::black_box(status);
        }
        Response::ContinuationRequest => {}
        Response::CommandData(untagged) => observe_untagged(untagged, &line),
    }
});

fn observe_untagged(mut untagged: UntaggedResponse, line: &[u8]) {
    std::hint::black_box((untagged.command_data, untagged.literal_data));
    let _ = untagged.parse_continue_line(line);
    std::hint::black_box(untagged.literal_data);
}

fn with_crlf(data: &[u8]) -> Cow<'_, [u8]> {
    if data.ends_with(b"\r\n") {
        Cow::Borrowed(data)
    } else {
        let mut line = Vec::with_capacity(data.len() + 2);
        line.extend_from_slice(data);
        line.extend_from_slice(b"\r\n");
        Cow::Owned(line)
    }
}
