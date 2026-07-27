/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io;
use tokio::io::{AsyncRead, ReadBuf};

pub struct ReadAllOnce<'a, R: ?Sized> {
    reader: &'a mut R,
    buf: &'a mut [u8],
}

impl<'a, R> ReadAllOnce<'a, R>
where
    R: AsyncRead + ?Sized + Unpin,
{
    pub(super) fn new(reader: &'a mut R, buf: &'a mut [u8]) -> Self {
        ReadAllOnce { reader, buf }
    }
}

fn read_all_once_internal<R: AsyncRead + ?Sized>(
    mut reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut [u8],
) -> Poll<io::Result<usize>> {
    let mut buf = ReadBuf::new(buf);
    let mut quit_on_pending = false;
    loop {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(buf.filled().len()));
        }
        let old_filled_len = buf.filled().len();
        match reader.as_mut().poll_read(cx, &mut buf) {
            Poll::Ready(Ok(_)) => {
                quit_on_pending = true;
                let filled_len = buf.filled().len();
                if filled_len == 0 {
                    return Poll::Ready(Ok(0));
                }
                if filled_len == old_filled_len {
                    return Poll::Ready(Ok(filled_len));
                }
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => {
                return if quit_on_pending {
                    Poll::Ready(Ok(buf.filled().len()))
                } else {
                    Poll::Pending
                };
            }
        }
    }
}

impl<R> Future for ReadAllOnce<'_, R>
where
    R: AsyncRead + ?Sized + Unpin,
{
    type Output = io::Result<usize>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ReadAllOnce { reader, buf } = &mut *self;
        read_all_once_internal(Pin::new(reader), cx, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_fills_buffer_once() {
        let mut stream = tokio_test::io::Builder::new().read(&b"abcd"[..]).build();
        let mut buf = [0u8; 4];
        let nr = ReadAllOnce::new(&mut stream, &mut buf).await.unwrap();
        assert_eq!(nr, 4);
        assert_eq!(&buf, b"abcd");
    }

    #[tokio::test]
    async fn read_returns_zero_on_closed_stream() {
        let mut stream = tokio_test::io::Builder::new().read(&[]).build();
        let mut buf = [0u8; 8];
        let nr = ReadAllOnce::new(&mut stream, &mut buf).await.unwrap();
        assert_eq!(nr, 0);
    }

    #[tokio::test]
    async fn read_stops_after_first_pending_with_partial_data() {
        let mut stream = tokio_test::io::Builder::new()
            .read(b"ab")
            .wait(std::time::Duration::from_secs(60))
            .build();
        let mut buf = [0u8; 8];
        let nr = ReadAllOnce::new(&mut stream, &mut buf).await.unwrap();
        assert_eq!(nr, 2);
        assert_eq!(&buf[..2], b"ab");
    }
}
