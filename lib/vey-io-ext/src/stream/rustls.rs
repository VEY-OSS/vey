/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::io;
use std::io::{Error, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf, ReadHalf, WriteHalf};
use tokio_rustls::{client, server};

use super::AsyncStream;

impl<S> AsyncStream for client::TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    type R = ReadHalf<client::TlsStream<S>>;
    type W = WriteHalf<client::TlsStream<S>>;

    fn into_split(self) -> (Self::R, Self::W) {
        tokio::io::split(self)
    }
}

impl<S> AsyncStream for server::TlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    type R = ReadHalf<server::TlsStream<S>>;
    type W = WriteHalf<server::TlsStream<S>>;

    fn into_split(self) -> (Self::R, Self::W) {
        tokio::io::split(self)
    }
}

pub enum MaybeTlsStreamReadHalf<S: AsyncStream> {
    Plain(S::R),
    Tls(ReadHalf<client::TlsStream<S>>),
}

impl<S> AsyncRead for MaybeTlsStreamReadHalf<S>
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
            MaybeTlsStreamReadHalf::Plain(reader) => Pin::new(reader).poll_read(cx, buf),
            MaybeTlsStreamReadHalf::Tls(tls_reader) => Pin::new(tls_reader).poll_read(cx, buf),
        }
    }
}

pub enum MaybeTlsStreamWriteHalf<S: AsyncStream> {
    Plain(S::W),
    Tls(WriteHalf<client::TlsStream<S>>),
}

impl<S> AsyncWrite for MaybeTlsStreamWriteHalf<S>
where
    S: AsyncStream + Unpin,
    S::W: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        match self.get_mut() {
            MaybeTlsStreamWriteHalf::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
            MaybeTlsStreamWriteHalf::Tls(tls_stream) => Pin::new(tls_stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            MaybeTlsStreamWriteHalf::Plain(stream) => Pin::new(stream).poll_flush(cx),
            MaybeTlsStreamWriteHalf::Tls(tls_stream) => Pin::new(tls_stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        match self.get_mut() {
            MaybeTlsStreamWriteHalf::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
            MaybeTlsStreamWriteHalf::Tls(tls_stream) => Pin::new(tls_stream).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<Result<usize, Error>> {
        match self.get_mut() {
            MaybeTlsStreamWriteHalf::Plain(stream) => {
                Pin::new(stream).poll_write_vectored(cx, bufs)
            }
            MaybeTlsStreamWriteHalf::Tls(tls_stream) => {
                Pin::new(tls_stream).poll_write_vectored(cx, bufs)
            }
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            MaybeTlsStreamWriteHalf::Plain(stream) => stream.is_write_vectored(),
            MaybeTlsStreamWriteHalf::Tls(tls_stream) => tls_stream.is_write_vectored(),
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum MaybeTlsStream<S> {
    Plain(S),
    Tls(client::TlsStream<S>),
}

impl<S> AsyncRead for MaybeTlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl<S> AsyncWrite for MaybeTlsStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_flush(cx),
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_shutdown(cx),
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_shutdown(cx),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            MaybeTlsStream::Tls(s) => Pin::new(s).poll_write_vectored(cx, bufs),
            MaybeTlsStream::Plain(s) => Pin::new(s).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            MaybeTlsStream::Tls(s) => s.is_write_vectored(),
            MaybeTlsStream::Plain(s) => s.is_write_vectored(),
        }
    }
}

impl<S> AsyncStream for MaybeTlsStream<S>
where
    S: AsyncStream + Unpin,
    S::R: AsyncRead + Unpin,
    S::W: AsyncWrite + Unpin,
{
    type R = MaybeTlsStreamReadHalf<S>;
    type W = MaybeTlsStreamWriteHalf<S>;

    fn into_split(self) -> (Self::R, Self::W) {
        match self {
            MaybeTlsStream::Plain(stream) => {
                let (r, w) = stream.into_split();
                (
                    MaybeTlsStreamReadHalf::Plain(r),
                    MaybeTlsStreamWriteHalf::Plain(w),
                )
            }
            MaybeTlsStream::Tls(tls_stream) => {
                let (r, w) = tls_stream.into_split();
                (
                    MaybeTlsStreamReadHalf::Tls(r),
                    MaybeTlsStreamWriteHalf::Tls(w),
                )
            }
        }
    }
}
