/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::io::IoSlice;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use bytes::{Buf, Bytes};
use pin_project_lite::pin_project;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, ReadBuf};

use super::DEFAULT_BUF_SIZE;
use crate::stream::{ArcLimitedReaderStats, AsyncStream, LimitedReader};
use crate::{GlobalLimitGroup, GlobalStreamLimit};

pin_project! {
    pub struct LimitedBufReader<R> {
        #[pin]
        inner: LimitedReader<R>,
        stats: ArcLimitedReaderStats,
        buf: Box<[MaybeUninit<u8>]>,
        pos: usize,
        cap: usize,
    }
}

impl<R> LimitedBufReader<R>
where
    R: AsyncRead,
{
    pub fn new(
        inner: R,
        shift_millis: u8,
        max_bytes: usize,
        direct_stats: ArcLimitedReaderStats,
        buffer_stats: ArcLimitedReaderStats,
    ) -> Self {
        LimitedBufReader::with_capacity(
            DEFAULT_BUF_SIZE,
            inner,
            shift_millis,
            max_bytes,
            direct_stats,
            buffer_stats,
        )
    }

    pub fn new_directed(from: LimitedReader<R>, buffer_stats: ArcLimitedReaderStats) -> Self {
        LimitedBufReader::directly_with_capacity(DEFAULT_BUF_SIZE, from, buffer_stats)
    }

    pub fn new_unlimited(
        inner: R,
        direct_stats: ArcLimitedReaderStats,
        buffer_stats: ArcLimitedReaderStats,
    ) -> Self {
        LimitedBufReader::unlimited_with_capacity(
            DEFAULT_BUF_SIZE,
            inner,
            direct_stats,
            buffer_stats,
        )
    }

    pub fn with_capacity(
        capacity: usize,
        inner: R,
        shift_millis: u8,
        max_bytes: usize,
        direct_stats: ArcLimitedReaderStats,
        buffer_stats: ArcLimitedReaderStats,
    ) -> Self {
        LimitedBufReader {
            inner: LimitedReader::local_limited(inner, shift_millis, max_bytes, direct_stats),
            stats: buffer_stats,
            buf: Box::new_uninit_slice(capacity),
            pos: 0,
            cap: 0,
        }
    }

    pub fn directly_with_capacity(
        capacity: usize,
        from: LimitedReader<R>,
        buffer_stats: ArcLimitedReaderStats,
    ) -> Self {
        LimitedBufReader {
            inner: from,
            stats: buffer_stats,
            buf: Box::new_uninit_slice(capacity),
            pos: 0,
            cap: 0,
        }
    }

    pub fn unlimited_with_capacity(
        capacity: usize,
        inner: R,
        direct_stats: ArcLimitedReaderStats,
        buffer_stats: ArcLimitedReaderStats,
    ) -> Self {
        LimitedBufReader {
            inner: LimitedReader::new(inner, direct_stats),
            stats: buffer_stats,
            buf: Box::new_uninit_slice(capacity),
            pos: 0,
            cap: 0,
        }
    }

    #[inline]
    pub fn reset_direct_stats(&mut self, stats: ArcLimitedReaderStats) {
        self.inner.reset_stats(stats);
    }

    #[inline]
    pub fn reset_buffer_stats(&mut self, stats: ArcLimitedReaderStats) {
        self.stats = stats;
    }

    #[inline]
    pub fn reset_local_limit(&mut self, shift_millis: u8, max_bytes: usize) {
        self.inner.reset_local_limit(shift_millis, max_bytes);
    }

    #[inline]
    pub fn add_global_limiter<T>(&mut self, limiter: Arc<T>)
    where
        T: GlobalStreamLimit + Send + Sync + 'static,
    {
        self.inner.add_global_limiter(limiter);
    }

    #[inline]
    pub fn retain_global_limiter_by_group(&mut self, group: GlobalLimitGroup) {
        self.inner.retain_global_limiter_by_group(group);
    }

    /// Consumes this reader, returning the underlying reader.
    ///
    /// Leftover data in the internal buffer is dropped.
    pub fn into_inner(self) -> R {
        self.inner.into_inner()
    }

    /// Splits off unread buffered bytes and the underlying reader.
    pub fn into_parts(self) -> (Bytes, R) {
        let inner = self.inner.into_inner();
        if self.pos < self.cap {
            // SAFETY: `buf[..cap]` was filled by `ReadBuf::uninit`; spare
            // capacity beyond `cap` stays unused.
            let vec = unsafe {
                let capacity = self.buf.len();
                let ptr = Box::into_raw(self.buf) as *mut u8;
                Vec::from_raw_parts(ptr, self.cap, capacity)
            };
            let mut bytes = Bytes::from(vec);
            bytes.advance(self.pos);
            (bytes, inner)
        } else {
            (Bytes::new(), inner)
        }
    }

    fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut LimitedReader<R>> {
        self.project().inner
    }
}

impl<R: AsyncRead> AsyncRead for LimitedBufReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.pos == self.cap && buf.remaining() >= self.buf.len() {
            let old_filled_len = buf.filled().len();
            let res = ready!(self.as_mut().get_pin_mut().poll_read(cx, buf));
            let nr = buf.filled().len() - old_filled_len;
            self.stats.add_read_bytes(nr);
            Poll::Ready(res)
        } else {
            let rem = ready!(self.as_mut().poll_fill_buf(cx))?;
            let amt = std::cmp::min(rem.len(), buf.remaining());
            buf.put_slice(&rem[..amt]);
            self.consume(amt);
            Poll::Ready(Ok(()))
        }
    }
}

impl<R: AsyncRead> AsyncBufRead for LimitedBufReader<R> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let me = self.project();
        if *me.pos >= *me.cap {
            let mut buf = ReadBuf::uninit(me.buf);
            ready!(me.inner.poll_read(cx, &mut buf))?;
            *me.cap = buf.filled().len();
            *me.pos = 0;
        }

        // SAFETY: `pos..cap` is initialized by `ReadBuf::uninit` fills.
        Poll::Ready(Ok(unsafe { me.buf[*me.pos..*me.cap].assume_init_ref() }))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let new_pos = std::cmp::min(self.pos + amt, self.cap);
        self.stats.add_read_bytes(new_pos - self.pos);
        let me = self.project();
        *me.pos = new_pos;
    }
}

impl<S: AsyncRead + AsyncWrite> AsyncWrite for LimitedBufReader<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

impl<S> AsyncStream for LimitedBufReader<S>
where
    S: AsyncStream,
    S::R: AsyncRead,
    S::W: AsyncWrite,
{
    type R = LimitedBufReader<S::R>;
    type W = S::W;

    fn into_split(self) -> (Self::R, Self::W) {
        let (r, w) = self.inner.into_split();
        (
            LimitedBufReader {
                inner: r,
                stats: self.stats,
                buf: self.buf,
                pos: self.pos,
                cap: self.cap,
            },
            w,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::io::AsyncReadExt;

    use super::*;
    use crate::NilLimitedStats;

    #[tokio::test]
    async fn into_parts_keeps_unread_buffer() {
        let inner = &b"hello-world"[..];
        let mut reader = LimitedBufReader::new_unlimited(
            inner,
            Arc::new(NilLimitedStats::default()),
            Arc::new(NilLimitedStats::default()),
        );
        let mut got = [0; 5];
        reader.read_exact(&mut got).await.unwrap();
        assert_eq!(&got, b"hello");

        let (buf, mut inner) = reader.into_parts();
        assert_eq!(&buf[..], b"-world");
        let mut rest = Vec::new();
        inner.read_to_end(&mut rest).await.unwrap();
        assert!(rest.is_empty());
    }
}
