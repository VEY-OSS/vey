/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::AsyncBufRead;

pub struct LimitedSkipUntil<'a, R: ?Sized> {
    reader: &'a mut R,
    delimiter: u8,
    read: usize,
    limit: usize,
}

impl<'a, R> LimitedSkipUntil<'a, R>
where
    R: AsyncBufRead + ?Sized + Unpin,
{
    pub(super) fn new(reader: &'a mut R, delimiter: u8, max_len: usize) -> Self {
        Self {
            reader,
            delimiter,
            read: 0,
            limit: max_len,
        }
    }
}

fn skip_until_internal<R: AsyncBufRead + ?Sized>(
    mut reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    delimiter: u8,
    read: &mut usize,
    limit: usize,
) -> Poll<io::Result<(bool, usize)>> {
    loop {
        let (done, used) = {
            let available = ready!(reader.as_mut().poll_fill_buf(cx))?;
            if let Some(i) = memchr::memchr(delimiter, available) {
                (true, i + 1)
            } else {
                (false, available.len())
            }
        };
        reader.as_mut().consume(used);
        *read += used;
        if done {
            return if *read > limit {
                Poll::Ready(Ok((false, mem::replace(read, 0))))
            } else {
                Poll::Ready(Ok((true, mem::replace(read, 0))))
            };
        }
        if used == 0 || *read > limit {
            return Poll::Ready(Ok((false, mem::replace(read, 0))));
        }
    }
}

impl<R: AsyncBufRead + ?Sized + Unpin> Future for LimitedSkipUntil<'_, R> {
    type Output = io::Result<(bool, usize)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Self {
            reader,
            delimiter,
            read,
            limit,
        } = &mut *self;
        skip_until_internal(Pin::new(reader), cx, *delimiter, read, *limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn skip_until_delimiter() {
        let content = b"junk prefix\r\nrest";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);

        let (found, size) = LimitedSkipUntil::new(&mut buf_stream, b'\n', 128)
            .await
            .unwrap();
        assert!(found);
        assert_eq!(size, 13);
    }

    #[tokio::test]
    async fn skip_until_exceeds_limit() {
        let content = b"0123456789abcdef\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);

        let (found, size) = LimitedSkipUntil::new(&mut buf_stream, b'\n', 8)
            .await
            .unwrap();
        assert!(!found);
        assert!(size >= 8);
    }

    #[tokio::test]
    async fn skip_until_eof_without_delimiter() {
        let content = b"no newline here";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);

        let (found, size) = LimitedSkipUntil::new(&mut buf_stream, b'\n', 128)
            .await
            .unwrap();
        assert!(!found);
        assert_eq!(size, content.len());
    }
}
