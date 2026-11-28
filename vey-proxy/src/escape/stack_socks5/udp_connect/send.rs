/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::task::{Context, Poll};

use slog::Logger;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use vey_io_ext::{AsUdpPayload, UdpCopyPacket};
use vey_io_ext::{UdpCopyRemoteError, UdpCopyRemoteSend};
use vey_socks::v5::SocksUdpHeader;
use vey_types::net::UpstreamAddr;

pub(crate) struct StackSocks5UdpConnectRemoteSend<T> {
    inner: T,
    socks5_header: SocksUdpHeader,
    scratch: Vec<u8>,
    logger: Option<Logger>,
}

impl<T> StackSocks5UdpConnectRemoteSend<T>
where
    T: UdpCopyRemoteSend,
{
    pub(crate) fn new(send: T, upstream: &UpstreamAddr, logger: Option<Logger>) -> Self {
        StackSocks5UdpConnectRemoteSend {
            inner: send,
            socks5_header: SocksUdpHeader::new(upstream),
            scratch: Vec::new(),
            logger,
        }
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
    fn frame_packets<B: AsUdpPayload>(&self, packets: &[B]) -> Vec<bytes::Bytes> {
        let header = self.socks5_header.as_ref();
        packets
            .iter()
            .map(|packet| {
                let payload = packet.as_payload();
                let mut buf = Vec::with_capacity(header.len() + payload.len());
                buf.extend_from_slice(header);
                buf.extend_from_slice(payload);
                bytes::Bytes::from(buf)
            })
            .collect()
    }
}

impl<T> UdpCopyRemoteSend for StackSocks5UdpConnectRemoteSend<T>
where
    T: UdpCopyRemoteSend + Unpin,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.scratch.clear();
        self.scratch.extend_from_slice(self.socks5_header.as_ref());
        self.scratch.extend_from_slice(buf);
        self.inner.poll_send_buf(cx, &self.scratch)
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
        let framed = self.frame_packets(packets);
        self.inner.poll_send_many_bytes(cx, &framed)
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
        let framed = self.frame_packets(packets);
        self.inner.poll_send_many_bytes(cx, &framed)
    }
}
