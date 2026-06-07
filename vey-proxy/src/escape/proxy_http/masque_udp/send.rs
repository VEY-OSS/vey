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

use crate::module::masque_udp::MasqueUdpSendBuffer;

pub(crate) struct ProxyHttpMasqueUdpSend<W> {
    buffer: MasqueUdpSendBuffer,
    writer: W,
    logger: Option<Logger>,
}

impl<W> ProxyHttpMasqueUdpSend<W>
where
    W: AsyncWrite + Unpin,
{
    pub(crate) fn new(writer: W, logger: Option<Logger>, max_packet_size: u16) -> Self {
        ProxyHttpMasqueUdpSend {
            buffer: MasqueUdpSendBuffer::new(max_packet_size),
            writer,
            logger,
        }
    }
}

impl<W> UdpCopyRemoteSend for ProxyHttpMasqueUdpSend<W>
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
        // The new packet will be dropped if a previous poll_send get canceled
        self.buffer.push_or_drop_packet(buf);
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
        // The new packet will be dropped if a previous poll_send get canceled
        self.buffer.push_or_drop_packets(packets);
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyRemoteError::SendFailed)
        )?;
        Poll::Ready(Ok(packets.len()))
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
        // The new packet will be dropped if a previous poll_send get canceled
        self.buffer.push_or_drop_many_bytes(packets);
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyRemoteError::SendFailed)
        )?;
        Poll::Ready(Ok(packets.len()))
    }
}
