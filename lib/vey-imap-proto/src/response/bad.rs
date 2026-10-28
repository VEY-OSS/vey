/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use tokio::io::AsyncWrite;

use vey_io_ext::LimitedWriteExt;

pub struct BadResponse {}

fn flatten_tag(tag: &str) -> String {
    tag.chars()
        .map(|c| if c.is_ascii_control() { ' ' } else { c })
        .collect()
}

impl BadResponse {
    pub async fn reply_invalid_command<W>(writer: &mut W, tag: &str) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        // A control byte in the tag would split this into another response line.
        let tag = flatten_tag(tag);
        let message = format!("{tag} BAD invalid command\r\n");
        writer.write_all_flush(message.as_bytes()).await
    }

    pub async fn reply_append_blocked<W>(writer: &mut W, tag: &str) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let tag = flatten_tag(tag);
        let message = format!("{tag} BAD the message is blocked\r\n");
        writer.write_all_flush(message.as_bytes()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bad_replies_include_tag() {
        let mut buf = Vec::new();
        BadResponse::reply_invalid_command(&mut buf, "A001")
            .await
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&buf).unwrap(),
            "A001 BAD invalid command\r\n"
        );

        buf.clear();
        BadResponse::reply_append_blocked(&mut buf, "A002")
            .await
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&buf).unwrap(),
            "A002 BAD the message is blocked\r\n"
        );
    }

    #[tokio::test]
    async fn bad_reply_keeps_one_line_when_tag_has_controls() {
        let tag = "A001\r\n\x01\x7f* OK injected";

        let mut buf = Vec::new();
        BadResponse::reply_invalid_command(&mut buf, tag)
            .await
            .unwrap();
        assert_one_bad_line(&buf);

        buf.clear();
        BadResponse::reply_append_blocked(&mut buf, tag)
            .await
            .unwrap();
        assert_one_bad_line(&buf);
    }

    fn assert_one_bad_line(bytes: &[u8]) {
        let mut lines = bytes.split_inclusive(|&b| b == b'\n');
        let line = lines.next().unwrap();
        assert!(lines.next().is_none());
        assert!(line.ends_with(b"\r\n"));
        assert!(line.windows(4).any(|w| w == b"A001"));
        assert!(line.windows(3).any(|w| w == b"BAD"));
        assert!(
            line[..line.len() - 2]
                .iter()
                .all(|b| (0x20..0x7f).contains(b))
        );
    }
}
