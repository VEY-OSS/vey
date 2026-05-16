/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::task::{Context, Poll, ready};

use thiserror::Error;

use super::UdpCopyPacket;

#[derive(Error, Debug)]
pub enum UdpCopyClientError {
    #[error("recv failed: {0:?}")]
    RecvFailed(io::Error),
    #[error("send failed: {0:?}")]
    SendFailed(io::Error),
    #[error("invalid packet: {0}")]
    InvalidPacket(String),
    #[error("mismatched client address")]
    MismatchedClientAddress,
    #[error("vary upstream")]
    VaryUpstream,
    #[error("forbidden client address")]
    ForbiddenClientAddress,
}

pub trait UdpCopyClientRecv {
    /// reserve some space for offloading header
    fn max_hdr_len(&self) -> usize;

    /// return `(off, len)`
    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyClientError>>;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut UdpCopyPacket,
    ) -> Poll<Result<(), UdpCopyClientError>> {
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
    ) -> Poll<Result<usize, UdpCopyClientError>>;
}

pub trait UdpCopyClientSend {
    /// return `nw`, which should be greater than 0
    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyClientError>>;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyClientError>>;
}
