/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::SocketAddr;
use std::task::{Context, Poll, ready};

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
use vey_io_ext::UdpRelayPacket;
use vey_io_ext::{UdpCopyRemoteSend, UdpRelayRemoteError, UdpRelayRemoteSend};
use vey_socks::v5::SocksUdpHeader;
use vey_types::net::UpstreamAddr;

pub(crate) struct StackSocks5UdpRelayRemoteSend<T> {
    inner: T,
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    header: SocksUdpHeader,
    scratch: Vec<u8>,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    headers: Vec<SocksUdpHeader>,
    logger: Option<Logger>,
}

impl<T> StackSocks5UdpRelayRemoteSend<T>
where
    T: UdpCopyRemoteSend,
{
    pub(crate) fn new(
        send: T,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
        logger: Option<Logger>,
    ) -> Self {
        StackSocks5UdpRelayRemoteSend {
            inner: send,
            local_addr,
            peer_addr,
            header: SocksUdpHeader::default(),
            scratch: Vec::new(),
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd",
                target_os = "macos",
                target_os = "solaris",
            ))]
            headers: Vec::new(),
            logger,
        }
    }

    fn send_failed(&self, error: impl ToString) -> UdpRelayRemoteError {
        UdpRelayRemoteError::SendFailed(
            self.local_addr,
            self.peer_addr,
            io::Error::other(error.to_string()),
        )
    }
}

impl<T> UdpRelayRemoteSend for StackSocks5UdpRelayRemoteSend<T>
where
    T: UdpCopyRemoteSend + Unpin,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
        to: &UpstreamAddr,
    ) -> Poll<Result<usize, UdpRelayRemoteError>> {
        let header = self.header.encode(to).to_vec();
        self.scratch.clear();
        self.scratch.extend_from_slice(&header);
        self.scratch.extend_from_slice(buf);
        self.inner
            .poll_send_buf(cx, &self.scratch)
            .map_err(|e| self.send_failed(e))
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
    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayRemoteError>> {
        if self.headers.len() < packets.len() {
            self.headers
                .resize(packets.len(), SocksUdpHeader::default());
        }
        let mut framed = Vec::with_capacity(packets.len());
        for (packet, header) in packets.iter().zip(self.headers.iter_mut()) {
            let hdr = header.encode(packet.upstream()).to_vec();
            let payload = packet.payload();
            let mut buf = Vec::with_capacity(hdr.len() + payload.len());
            buf.extend_from_slice(&hdr);
            buf.extend_from_slice(payload);
            framed.push(bytes::Bytes::from(buf));
        }
        Poll::Ready(
            ready!(self.inner.poll_send_many_bytes(cx, &framed)).map_err(|e| self.send_failed(e)),
        )
    }
}
