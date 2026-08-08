/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use tokio::io::AsyncWrite;

use vey_io_ext::LimitedWriteExt;

pub struct BadResponse {}

impl BadResponse {
    pub async fn reply_invalid_command<W>(writer: &mut W, tag: &str) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let message = format!("{tag} BAD invalid command\r\n");
        writer.write_all_flush(message.as_bytes()).await
    }

    pub async fn reply_append_blocked<W>(writer: &mut W, tag: &str) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
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
}
