/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::task::{Context, Poll, ready};

#[cfg(feature = "log")]
use slog::Logger;
use thiserror::Error;

use super::UdpCopyPacket;

#[derive(Error, Debug)]
pub enum UdpCopyRemoteError {
    #[error("recv failed: {0:?}")]
    RecvFailed(io::Error),
    #[error("send failed: {0:?}")]
    SendFailed(io::Error),
    #[error("invalid packet: {0}")]
    InvalidPacket(String),
    #[error("remote session closed")]
    RemoteSessionClosed,
    #[error("remote session error: {0:?}")]
    RemoteSessionError(io::Error),
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
}

pub trait UdpCopyRemoteRecv {
    #[cfg(feature = "log")]
    fn error_logger(&self) -> Option<&Logger>;

    /// reserve some space for offloading header
    fn max_hdr_len(&self) -> usize;

    /// return `(off, len. from)`
    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyRemoteError>>;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut UdpCopyPacket,
    ) -> Poll<Result<(), UdpCopyRemoteError>> {
        let (off, len) = ready!(self.poll_recv_buf(cx, buf.buf_mut()))?;
        buf.set_length(len);
        buf.set_offset(off);
        Poll::Ready(Ok(()))
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyRemoteError>>;
}

impl<T: ?Sized + UdpCopyRemoteRecv> UdpCopyRemoteRecv for Box<T> {
    #[cfg(feature = "log")]
    fn error_logger(&self) -> Option<&Logger> {
        self.as_ref().error_logger()
    }

    fn max_hdr_len(&self) -> usize {
        self.as_ref().max_hdr_len()
    }

    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyRemoteError>> {
        self.as_mut().poll_recv_buf(cx, buf)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.as_mut().poll_recv_packets(cx, packets)
    }
}

pub trait UdpCopyRemoteSend {
    #[cfg(feature = "log")]
    fn error_logger(&self) -> Option<&Logger>;

    /// return `nw`, which should be greater than 0
    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyRemoteError>>;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_send_many_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyRemoteError>>;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_send_many_bytes(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[bytes::Bytes],
    ) -> Poll<Result<usize, UdpCopyRemoteError>>;
}

impl<T: ?Sized + UdpCopyRemoteSend> UdpCopyRemoteSend for Box<T> {
    #[cfg(feature = "log")]
    fn error_logger(&self) -> Option<&Logger> {
        self.as_ref().error_logger()
    }

    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.as_mut().poll_send_buf(cx, buf)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_send_many_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.as_mut().poll_send_many_packets(cx, packets)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_send_many_bytes(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[bytes::Bytes],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.as_mut().poll_send_many_bytes(cx, packets)
    }
}
