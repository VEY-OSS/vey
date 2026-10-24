/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

use crate::{StreamCopyConfig, StreamCopyError};

pub struct BufReadCopy<'a, R: ?Sized, W: ?Sized> {
    reader: &'a mut R,
    writer: &'a mut W,
    yield_size: usize,
    total_write: u64,
    buf_size: usize,
    read_done: bool,
    need_flush: bool,
    active: bool,
}

impl<'a, R, W> BufReadCopy<'a, R, W>
where
    R: AsyncBufRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    pub fn new(reader: &'a mut R, writer: &'a mut W, config: &StreamCopyConfig) -> Self {
        BufReadCopy {
            reader,
            writer,
            yield_size: config.yield_size(),
            total_write: 0,
            buf_size: 0,
            read_done: false,
            need_flush: false,
            active: false,
        }
    }

    #[inline]
    pub fn no_cached_data(&self) -> bool {
        self.buf_size == 0
    }

    #[inline]
    pub fn finished(&self) -> bool {
        self.read_done
    }

    #[inline]
    pub fn copied_size(&self) -> u64 {
        self.total_write
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.active
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        !self.active
    }

    #[inline]
    pub fn reset_active(&mut self) {
        self.active = false;
    }

    fn poll_write_cache(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), StreamCopyError>> {
        loop {
            match Pin::new(&mut self.reader).poll_fill_buf(cx) {
                Poll::Ready(Ok(buf)) => {
                    self.buf_size = buf.len();
                    if buf.is_empty() {
                        self.read_done = true;
                        return Poll::Ready(Ok(()));
                    }
                    let i = ready!(Pin::new(&mut self.writer).poll_write(cx, buf))
                        .map_err(StreamCopyError::WriteFailed)?;
                    if i == 0 {
                        return Poll::Ready(Err(StreamCopyError::WriteFailed(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "write zero",
                        ))));
                    }
                    self.need_flush = true;
                    self.active = true;
                    self.buf_size -= i;
                    self.total_write += i as u64;
                    Pin::new(&mut *self.reader).consume(i);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(StreamCopyError::ReadFailed(e))),
                Poll::Pending => return Poll::Ready(Ok(())),
            }
        }
    }

    pub async fn write_flush(&mut self) -> Result<(), StreamCopyError> {
        if self.read_done {
            return Ok(());
        }

        if self.buf_size > 0 {
            poll_fn(|cx| self.poll_write_cache(cx)).await?;
        }

        if self.need_flush {
            self.writer
                .flush()
                .await
                .map_err(StreamCopyError::WriteFailed)?;
        }

        Ok(())
    }
}

impl<R, W> Future for BufReadCopy<'_, R, W>
where
    R: AsyncBufRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = Result<u64, StreamCopyError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut copy_this_round = 0;
        loop {
            let me = &mut *self;
            let buffer = match Pin::new(&mut *me.reader).poll_fill_buf(cx) {
                Poll::Ready(Ok(buffer)) => {
                    me.buf_size = buffer.len();
                    if buffer.is_empty() {
                        if self.need_flush {
                            ready!(Pin::new(&mut self.writer).poll_flush(cx))
                                .map_err(StreamCopyError::WriteFailed)?;
                        }
                        self.read_done = true;
                        return Poll::Ready(Ok(self.total_write));
                    }
                    buffer
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(StreamCopyError::ReadFailed(e))),
                Poll::Pending => {
                    if self.need_flush {
                        ready!(Pin::new(&mut self.writer).poll_flush(cx))
                            .map_err(StreamCopyError::WriteFailed)?;
                    }
                    return Poll::Pending;
                }
            };

            let i = ready!(Pin::new(&mut *me.writer).poll_write(cx, buffer))
                .map_err(StreamCopyError::WriteFailed)?;
            if i == 0 {
                return Poll::Ready(Err(StreamCopyError::WriteFailed(
                    io::ErrorKind::WriteZero.into(),
                )));
            }
            self.need_flush = true;
            self.active = true;
            self.buf_size -= i;
            self.total_write += i as u64;
            Pin::new(&mut *self.reader).consume(i);

            copy_this_round += i;
            if copy_this_round >= self.yield_size {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};
    use std::time::Duration;

    struct ZeroWriter;

    impl AsyncWrite for ZeroWriter {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            _buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(0))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    fn assert_write_zero(err: StreamCopyError) {
        match err {
            StreamCopyError::WriteFailed(err) => assert_eq!(err.kind(), io::ErrorKind::WriteZero),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn write_zero_stops_the_copy() {
        let mut reader = std::io::Cursor::new(&b"abc"[..]);
        let mut writer = ZeroWriter;
        let copy = BufReadCopy::new(&mut reader, &mut writer, &StreamCopyConfig::default());
        let err = tokio::time::timeout(Duration::from_secs(1), copy)
            .await
            .expect("write zero must not spin")
            .unwrap_err();
        assert_write_zero(err);
    }

    #[tokio::test]
    async fn write_zero_stops_poll_write_cache() {
        let mut reader = std::io::Cursor::new(&b"abc"[..]);
        let mut writer = ZeroWriter;
        let mut copy = BufReadCopy::new(&mut reader, &mut writer, &StreamCopyConfig::default());
        copy.buf_size = 1;
        let err = tokio::time::timeout(Duration::from_secs(1), copy.write_flush())
            .await
            .expect("write zero must not spin")
            .unwrap_err();
        assert_write_zero(err);
    }
}
