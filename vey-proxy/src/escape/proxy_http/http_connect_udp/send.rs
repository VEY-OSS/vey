/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll, ready};

use slog::Logger;
use tokio::io::AsyncWrite;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use vey_io_ext::UdpCopyPacket;
use vey_io_ext::{UdpCopyRemoteError, UdpCopyRemoteSend};

use crate::module::http_connect_udp::{HttpConnectUdpSendBuffer, PacketTooLarge};

pub(crate) struct ProxyHttpConnectUdpSend<W> {
    buffer: HttpConnectUdpSendBuffer,
    writer: W,
    logger: Option<Logger>,
}

impl<W> ProxyHttpConnectUdpSend<W>
where
    W: AsyncWrite + Unpin,
{
    pub(crate) fn new(writer: W, logger: Option<Logger>, max_packet_size: u16) -> Self {
        ProxyHttpConnectUdpSend {
            buffer: HttpConnectUdpSendBuffer::new(max_packet_size),
            writer,
            logger,
        }
    }
}

impl<W> UdpCopyRemoteSend for ProxyHttpConnectUdpSend<W>
where
    W: AsyncWrite + Unpin,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.buffer
            .queue_packet(buf)
            .map_err(|PacketTooLarge| {
                UdpCopyRemoteError::InvalidPacket(format!(
                    "UDP packet length {} exceeds max {}",
                    buf.len(),
                    self.buffer.max_packet_size()
                ))
            })?;
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyRemoteError::SendFailed)
        )?;
        Poll::Ready(Ok(buf.len()))
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
        let count = self
            .buffer
            .queue_packets(packets)
            .map_err(|PacketTooLarge| {
                UdpCopyRemoteError::InvalidPacket(format!(
                    "UDP packet exceeds max {}",
                    self.buffer.max_packet_size()
                ))
            })?;
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyRemoteError::SendFailed)
        )?;
        Poll::Ready(Ok(count))
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
        let count = self
            .buffer
            .queue_many_bytes(packets)
            .map_err(|PacketTooLarge| {
                UdpCopyRemoteError::InvalidPacket(format!(
                    "UDP packet exceeds max {}",
                    self.buffer.max_packet_size()
                ))
            })?;
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyRemoteError::SendFailed)
        )?;
        Poll::Ready(Ok(count))
    }
}
