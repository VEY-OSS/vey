/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll, ready};

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
use vey_io_ext::{UdpCopyClientError, UdpCopyClientSend};

use crate::module::masque_udp::MasqueUdpSendBuffer;

pub(super) struct MasqueUdpSend<W> {
    buffer: MasqueUdpSendBuffer,
    writer: W,
}

impl<W> MasqueUdpSend<W>
where
    W: AsyncWrite + Unpin,
{
    pub(super) fn new(writer: W, max_packet_size: u16) -> Self {
        MasqueUdpSend {
            buffer: MasqueUdpSendBuffer::new(max_packet_size),
            writer,
        }
    }
}

impl<W> UdpCopyClientSend for MasqueUdpSend<W>
where
    W: AsyncWrite + Unpin,
{
    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        // The new packet will be dropped if a previous poll_send get canceled
        self.buffer.push_or_drop_packet(buf);
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyClientError::SendFailed)
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
    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        // The new packet will be dropped if a previous poll_send get canceled
        self.buffer.push_or_drop_packets(packets);
        ready!(
            self.buffer
                .poll_write(cx, Pin::new(&mut self.writer))
                .map_err(UdpCopyClientError::SendFailed)
        )?;
        Poll::Ready(Ok(packets.len()))
    }
}
