/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll, ready};

use tokio::io::AsyncRead;

use vey_io_ext::{UdpCopyClientError, UdpCopyClientRecv, UdpCopyPacket};

use crate::module::masque_udp::MasqueUdpRecvBuffer;

pub(super) struct MasqueUdpRecv<R> {
    reader: R,
    buffer: MasqueUdpRecvBuffer,
}

impl<R> MasqueUdpRecv<R>
where
    R: AsyncRead + Unpin,
{
    pub(super) fn new(reader: R, capacity: usize, max_packet_size: u16) -> Self {
        MasqueUdpRecv {
            reader,
            buffer: MasqueUdpRecvBuffer::new(capacity, max_packet_size),
        }
    }

    fn poll_datagram(&mut self, cx: &mut Context<'_>) -> Poll<Result<&[u8], UdpCopyClientError>> {
        let buf = ready!(self.buffer.poll_datagram(cx, Pin::new(&mut self.reader)))?;
        Poll::Ready(Ok(buf))
    }
}

impl<R> UdpCopyClientRecv for MasqueUdpRecv<R>
where
    R: AsyncRead + Unpin,
{
    fn max_hdr_len(&self) -> usize {
        // the capsule header is parsed in local buf, so we don't need to reserve extra space for it
        0
    }

    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyClientError>> {
        let datagram = ready!(self.poll_datagram(cx))?;
        if datagram.is_empty() {
            self.buffer.consume_datagram();
            return Poll::Ready(Ok((0, 0)));
        }
        let copy_len = datagram.len().min(buf.len());
        unsafe {
            std::ptr::copy_nonoverlapping(datagram.as_ptr(), buf.as_mut_ptr(), copy_len);
        }
        self.buffer.consume_datagram();
        Poll::Ready(Ok((0, copy_len)))
    }

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), UdpCopyClientError>> {
        let datagram = ready!(self.poll_datagram(cx))?;
        if datagram.is_empty() {
            packet.set_offset(0);
            packet.set_length(0);
            self.buffer.consume_datagram();
            return Poll::Ready(Ok(()));
        }
        let fill_buf = packet.buf_mut();
        let copy_len = datagram.len().min(fill_buf.len());
        unsafe {
            std::ptr::copy_nonoverlapping(datagram.as_ptr(), fill_buf.as_mut_ptr(), copy_len);
        }
        self.buffer.consume_datagram();
        packet.set_offset(0);
        packet.set_length(copy_len);
        Poll::Ready(Ok(()))
    }
}
