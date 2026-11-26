/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use bytes::Bytes;
use h2::RecvStream;
use tokio::io::AsyncWrite;

use super::to_chunked_transfer::H2StreamToChunkedTransferError;

/// Copy an HTTP/2 request body onto an HTTP/1.1 connection as raw bytes.
///
/// Used when the converted request keeps `Content-Length` and therefore must
/// not be wrapped in chunked transfer encoding. Trailer fields, if any arrive,
/// are drained and dropped: a content-length body cannot carry them.
pub struct H2StreamToRawTransfer<'a, W> {
    recv_stream: &'a mut RecvStream,
    writer: &'a mut W,
    yield_size: usize,
    pending: Option<Bytes>,
    copied: u64,
    read_data_finished: bool,
    trailers_done: bool,
    active: bool,
    finished: bool,
}

impl<'a, W> H2StreamToRawTransfer<'a, W> {
    pub fn new(recv_stream: &'a mut RecvStream, writer: &'a mut W, yield_size: usize) -> Self {
        H2StreamToRawTransfer {
            recv_stream,
            writer,
            yield_size,
            pending: None,
            copied: 0,
            read_data_finished: false,
            trailers_done: false,
            active: false,
            finished: false,
        }
    }

    pub fn with_prefix(
        recv_stream: &'a mut RecvStream,
        writer: &'a mut W,
        yield_size: usize,
        prefix: Bytes,
    ) -> Self {
        H2StreamToRawTransfer {
            recv_stream,
            writer,
            yield_size,
            pending: if prefix.is_empty() {
                None
            } else {
                Some(prefix)
            },
            copied: 0,
            read_data_finished: false,
            trailers_done: false,
            active: false,
            finished: false,
        }
    }

    pub fn finished(&self) -> bool {
        self.finished
    }

    #[inline]
    pub fn recv_finished(&self) -> bool {
        self.read_data_finished
    }

    pub fn is_idle(&self) -> bool {
        !self.active
    }

    pub fn reset_active(&mut self) {
        self.active = false;
    }

    pub fn no_cached_data(&self) -> bool {
        self.pending.is_none()
    }

    #[inline]
    pub fn copied_size(&self) -> u64 {
        self.copied
    }

    #[inline]
    pub fn received_size(&self) -> u64 {
        self.copied + self.pending.as_ref().map(|c| c.len() as u64).unwrap_or(0)
    }

    pub fn add_copied(&mut self, n: u64) {
        self.copied += n;
    }

    fn poll_transfer(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<u64, H2StreamToChunkedTransferError>>
    where
        W: AsyncWrite + Unpin,
    {
        if self.finished {
            return Poll::Ready(Ok(self.copied));
        }

        let mut copy_this_round = 0usize;
        loop {
            if self.pending.is_none() && !self.read_data_finished {
                match ready!(self.recv_stream.poll_data(cx)) {
                    Some(Ok(chunk)) => {
                        self.active = true;
                        if chunk.is_empty() {
                            continue;
                        }
                        let nr = chunk.len();
                        self.recv_stream
                            .flow_control()
                            .release_capacity(nr)
                            .map_err(H2StreamToChunkedTransferError::RecvDataFailed)?;
                        self.pending = Some(chunk);
                    }
                    Some(Err(e)) => {
                        return Poll::Ready(Err(H2StreamToChunkedTransferError::RecvDataFailed(e)));
                    }
                    None => {
                        self.read_data_finished = true;
                        self.active = true;
                    }
                }
            }

            while let Some(mut chunk) = self.pending.take() {
                let writer = Pin::new(&mut *self.writer);
                match writer.poll_write(cx, &chunk) {
                    Poll::Ready(Ok(0)) => {
                        self.pending = Some(chunk);
                        return Poll::Ready(Err(H2StreamToChunkedTransferError::WriteError(
                            io::Error::new(io::ErrorKind::WriteZero, "zero byte written out"),
                        )));
                    }
                    Poll::Ready(Ok(nw)) => {
                        let left = chunk.split_off(nw);
                        self.copied += nw as u64;
                        copy_this_round += nw;
                        self.active = true;
                        if !left.is_empty() {
                            self.pending = Some(left);
                        }
                    }
                    Poll::Ready(Err(e)) => {
                        self.pending = Some(chunk);
                        return Poll::Ready(Err(H2StreamToChunkedTransferError::WriteError(e)));
                    }
                    Poll::Pending => {
                        self.pending = Some(chunk);
                        return Poll::Pending;
                    }
                }
            }

            if self.read_data_finished {
                if !self.trailers_done {
                    match ready!(self.recv_stream.poll_trailers(cx)) {
                        Ok(_) => {
                            self.trailers_done = true;
                            self.active = true;
                        }
                        Err(e) => {
                            return Poll::Ready(Err(
                                H2StreamToChunkedTransferError::RecvTrailerFailed(e),
                            ));
                        }
                    }
                }
                ready!(Pin::new(&mut *self.writer).poll_flush(cx))
                    .map_err(H2StreamToChunkedTransferError::WriteError)?;
                self.finished = true;
                return Poll::Ready(Ok(self.copied));
            }

            if copy_this_round >= self.yield_size {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        }
    }
}

impl<W> Future for H2StreamToRawTransfer<'_, W>
where
    W: AsyncWrite + Unpin,
{
    type Output = Result<u64, H2StreamToChunkedTransferError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.get_mut().poll_transfer(cx)
    }
}
