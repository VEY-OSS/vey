/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use atoi::FromRadix10Checked;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// Parse the byte offset value in the `Encapsulated` header.
pub(crate) fn parse_offset(value: &str) -> Option<usize> {
    if value.is_empty() {
        return None;
    }
    let (offset, len) = usize::from_radix_10_checked(value.as_bytes());
    if len != value.len() {
        return None;
    }
    offset
}

/// Skip the encapsulated part that is not needed.
pub(crate) async fn skip_bytes<R>(reader: &mut R, mut size: usize) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
{
    while size > 0 {
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed while skipping encapsulated data",
            ));
        }
        let n = buf.len().min(size);
        reader.consume(n);
        size -= n;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parse_offset_values() {
        assert_eq!(parse_offset("0"), Some(0));
        assert_eq!(parse_offset("128"), Some(128));
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("1x"), None);
        assert_eq!(parse_offset("-1"), None);
        assert_eq!(parse_offset("99999999999999999999999999"), None);
    }

    #[tokio::test]
    async fn skip_bytes_consumes_exact_size() {
        let mut reader = Cursor::new(&b"abcdef"[..]);
        skip_bytes(&mut reader, 4).await.unwrap();
        let left = reader.fill_buf().await.unwrap();
        assert_eq!(left, b"ef");
    }

    #[tokio::test]
    async fn skip_bytes_fails_on_eof() {
        let mut reader = Cursor::new(&b"ab"[..]);
        let e = skip_bytes(&mut reader, 4).await.unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof);
    }
}
