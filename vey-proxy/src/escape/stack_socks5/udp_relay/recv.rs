/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use slog::Logger;
use tokio::io::{AsyncRead, ReadBuf};

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use vey_io_ext::{UdpCopyPacket, UdpRelayPacket, UdpRelayPacketMeta};
use vey_io_ext::{UdpCopyRemoteRecv, UdpRelayRemoteError, UdpRelayRemoteRecv};
use vey_socks::v5::UdpInput;
use vey_types::net::UpstreamAddr;

pub(crate) struct StackSocks5UdpRelayRemoteRecv<T, C> {
    local_addr: SocketAddr,
    peer_addr: SocketAddr,
    inner: T,
    ctl_stream: C,
    end_on_control_closed: bool,
    ignore_ctl_stream: bool,
    logger: Option<Logger>,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    recv_packets: Vec<UdpCopyPacket>,
}

impl<T, C> StackSocks5UdpRelayRemoteRecv<T, C>
where
    T: UdpCopyRemoteRecv,
    C: AsyncRead + Unpin,
{
    pub(crate) fn new(
        recv: T,
        local_addr: SocketAddr,
        peer_addr: SocketAddr,
        ctl_stream: C,
        end_on_control_closed: bool,
        logger: Option<Logger>,
    ) -> Self {
        StackSocks5UdpRelayRemoteRecv {
            local_addr,
            peer_addr,
            inner: recv,
            ctl_stream,
            end_on_control_closed,
            ignore_ctl_stream: false,
            logger,
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd",
                target_os = "macos",
                target_os = "solaris",
            ))]
            recv_packets: Vec::new(),
        }
    }

    fn check_tcp_close(&mut self, cx: &mut Context<'_>) -> Result<(), UdpRelayRemoteError> {
        const MAX_MSG_SIZE: usize = 4;
        let mut buf = [0u8; MAX_MSG_SIZE];

        let mut read_buf = ReadBuf::new(&mut buf);
        match Pin::new(&mut self.ctl_stream).poll_read(cx, &mut read_buf) {
            Poll::Pending => Ok(()),
            Poll::Ready(Ok(_)) => match read_buf.filled().len() {
                0 => {
                    if self.end_on_control_closed {
                        Err(UdpRelayRemoteError::RemoteSessionClosed(
                            self.local_addr,
                            self.peer_addr,
                        ))
                    } else {
                        self.ignore_ctl_stream = true;
                        Ok(())
                    }
                }
                MAX_MSG_SIZE => Err(UdpRelayRemoteError::RemoteSessionError(
                    self.local_addr,
                    self.peer_addr,
                    io::Error::other("unexpected data received in ctl stream"),
                )),
                _ => Ok(()), // drain extra data sent by some bad implementation
            },
            Poll::Ready(Err(e)) => Err(UdpRelayRemoteError::RemoteSessionError(
                self.local_addr,
                self.peer_addr,
                e,
            )),
        }
    }
}

impl<T, C> UdpRelayRemoteRecv for StackSocks5UdpRelayRemoteRecv<T, C>
where
    T: UdpCopyRemoteRecv + Unpin,
    C: AsyncRead + Unpin,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn max_hdr_len(&self) -> usize {
        self.inner.max_hdr_len() + 256 + 4 + 2
    }

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize, UpstreamAddr), UdpRelayRemoteError>> {
        if !self.ignore_ctl_stream {
            self.check_tcp_close(cx)?;
        }
        self.poll_recv_packet_data(cx, buf)
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
        packets: &mut [UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayRemoteError>> {
        if !self.ignore_ctl_stream {
            self.check_tcp_close(cx)?;
        }

        self.prepare_recv_packets(packets);
        let count = ready!(
            self.inner
                .poll_recv_packets(cx, &mut self.recv_packets[..packets.len()],)
        )
        .map_err(|e| {
            UdpRelayRemoteError::RecvFailed(self.local_addr, io::Error::other(e.to_string()))
        })?;

        let mut parsed = Vec::with_capacity(count);
        for src in self.recv_packets.iter().take(count) {
            let payload = src.payload();
            let (header_len, upstream) = UdpInput::parse_header(payload)
                .map_err(|e| UdpRelayRemoteError::InvalidPacket(self.local_addr, e.to_string()))?;
            parsed.push((header_len, payload.to_vec(), upstream));
        }
        for ((header_len, datagram, upstream), packet) in parsed.into_iter().zip(packets.iter_mut())
        {
            let end = datagram.len();
            packet.buf_mut()[..end].copy_from_slice(&datagram);
            let iov = IoSliceMut::new(&mut packet.buf_mut()[..end]);
            UdpRelayPacketMeta::new(&iov, header_len, end, upstream).set_packet(packet);
        }

        self.end_on_control_closed = true;
        Poll::Ready(Ok(count))
    }
}

impl<T, C> StackSocks5UdpRelayRemoteRecv<T, C>
where
    T: UdpCopyRemoteRecv + Unpin,
    C: AsyncRead + Unpin,
{
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn prepare_recv_packets(&mut self, packets: &[UdpRelayPacket]) {
        let same = self.recv_packets.len() == packets.len()
            && self
                .recv_packets
                .iter()
                .zip(packets)
                .all(|(src, dst)| src.buf_len() == dst.buf().len());
        if same {
            return;
        }
        self.recv_packets = packets
            .iter()
            .map(|packet| {
                let len = packet.buf().len().min(u16::MAX as usize) as u16;
                UdpCopyPacket::new(0, len)
            })
            .collect();
    }

    fn poll_recv_packet_data(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize, UpstreamAddr), UdpRelayRemoteError>> {
        let (off, end) = ready!(self.inner.poll_recv_buf(cx, buf)).map_err(|e| {
            UdpRelayRemoteError::RecvFailed(self.local_addr, io::Error::other(e.to_string()))
        })?;
        let (header_len, upstream) = UdpInput::parse_header(&buf[off..end])
            .map_err(|e| UdpRelayRemoteError::InvalidPacket(self.local_addr, e.to_string()))?;
        self.end_on_control_closed = true;
        Poll::Ready(Ok((off + header_len, end, upstream)))
    }
}
