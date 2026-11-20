/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::io::{self, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, ReadHalf, WriteHalf};

use vey_openssl::SslStream;

use super::AsyncStream;

impl<S> AsyncStream for SslStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    type R = ReadHalf<SslStream<S>>;
    type W = WriteHalf<SslStream<S>>;

    fn into_split(self) -> (Self::R, Self::W) {
        tokio::io::split(self)
    }
}

pub enum MaybeSslStreamReadHalf<S: AsyncStream> {
    Plain(S::R),
    Ssl(ReadHalf<SslStream<S>>),
}

impl<S> AsyncRead for MaybeSslStreamReadHalf<S>
where
    S: AsyncStream + Unpin,
    S::R: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeSslStreamReadHalf::Ssl(s) => Pin::new(s).poll_read(cx, buf),
            MaybeSslStreamReadHalf::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

pub enum MaybeSslStreamWriteHalf<S: AsyncStream> {
    Plain(S::W),
    Ssl(WriteHalf<SslStream<S>>),
}

impl<S> AsyncWrite for MaybeSslStreamWriteHalf<S>
where
    S: AsyncStream + Unpin,
    S::W: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeSslStreamWriteHalf::Ssl(s) => Pin::new(s).poll_write(cx, buf),
            MaybeSslStreamWriteHalf::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeSslStreamWriteHalf::Ssl(s) => Pin::new(s).poll_flush(cx),
            MaybeSslStreamWriteHalf::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeSslStreamWriteHalf::Ssl(s) => Pin::new(s).poll_shutdown(cx),
            MaybeSslStreamWriteHalf::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeSslStreamWriteHalf::Ssl(s) => Pin::new(s).poll_write_vectored(cx, bufs),
            MaybeSslStreamWriteHalf::Plain(s) => Pin::new(s).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            MaybeSslStreamWriteHalf::Ssl(s) => s.is_write_vectored(),
            MaybeSslStreamWriteHalf::Plain(s) => s.is_write_vectored(),
        }
    }
}

pub enum MaybeSslStream<S> {
    Plain(S),
    Ssl(SslStream<S>),
}

impl<S> AsyncRead for MaybeSslStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeSslStream::Ssl(s) => Pin::new(s).poll_read(cx, buf),
            MaybeSslStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl<S> AsyncWrite for MaybeSslStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeSslStream::Ssl(s) => Pin::new(s).poll_write(cx, buf),
            MaybeSslStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeSslStream::Ssl(s) => Pin::new(s).poll_flush(cx),
            MaybeSslStream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeSslStream::Ssl(s) => Pin::new(s).poll_shutdown(cx),
            MaybeSslStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeSslStream::Ssl(s) => Pin::new(s).poll_write_vectored(cx, bufs),
            MaybeSslStream::Plain(s) => Pin::new(s).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            MaybeSslStream::Ssl(s) => s.is_write_vectored(),
            MaybeSslStream::Plain(s) => s.is_write_vectored(),
        }
    }
}

impl<S> AsyncStream for MaybeSslStream<S>
where
    S: AsyncStream + Unpin,
    S::R: AsyncRead + Unpin,
    S::W: AsyncWrite + Unpin,
{
    type R = MaybeSslStreamReadHalf<S>;
    type W = MaybeSslStreamWriteHalf<S>;

    fn into_split(self) -> (Self::R, Self::W) {
        match self {
            MaybeSslStream::Plain(stream) => {
                let (r, w) = stream.into_split();
                (
                    MaybeSslStreamReadHalf::Plain(r),
                    MaybeSslStreamWriteHalf::Plain(w),
                )
            }
            MaybeSslStream::Ssl(ssl_stream) => {
                let (r, w) = ssl_stream.into_split();
                (
                    MaybeSslStreamReadHalf::Ssl(r),
                    MaybeSslStreamWriteHalf::Ssl(w),
                )
            }
        }
    }
}
