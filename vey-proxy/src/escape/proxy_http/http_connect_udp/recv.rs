/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll, ready};

use slog::Logger;
use tokio::io::AsyncRead;

use vey_io_ext::{UdpCopyPacket, UdpCopyRemoteError, UdpCopyRemoteRecv};

use crate::module::http_connect_udp::HttpConnectUdpRecvBuffer;

pub(crate) struct ProxyHttpConnectUdpRecv<R> {
    reader: R,
    logger: Option<Logger>,
    buffer: HttpConnectUdpRecvBuffer,
}

impl<R> ProxyHttpConnectUdpRecv<R>
where
    R: AsyncRead + Unpin,
{
    pub(crate) fn new(
        reader: R,
        logger: Option<Logger>,
        capacity: usize,
        max_packet_size: u16,
    ) -> Self {
        ProxyHttpConnectUdpRecv {
            reader,
            logger,
            buffer: HttpConnectUdpRecvBuffer::new(capacity, max_packet_size),
        }
    }

    fn poll_datagram(&mut self, cx: &mut Context<'_>) -> Poll<Result<&[u8], UdpCopyRemoteError>> {
        let buf = ready!(self.buffer.poll_datagram(cx, Pin::new(&mut self.reader)))?;
        Poll::Ready(Ok(buf))
    }
}

impl<R> UdpCopyRemoteRecv for ProxyHttpConnectUdpRecv<R>
where
    R: AsyncRead + Unpin,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn max_hdr_len(&self) -> usize {
        // the capsule header is parsed in local buf, so we don't need to reserve extra space for it
        0
    }

    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyRemoteError>> {
        let datagram = ready!(self.poll_datagram(cx))?;
        if datagram.is_empty() {
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
        buf: &mut UdpCopyPacket,
    ) -> Poll<Result<(), UdpCopyRemoteError>> {
        let datagram = ready!(self.poll_datagram(cx))?;
        if datagram.is_empty() {
            buf.set_offset(0);
            buf.set_length(0);
            self.buffer.consume_datagram();
            return Poll::Ready(Ok(()));
        }
        let fill_buf = buf.buf_mut();
        let copy_len = datagram.len().min(fill_buf.len());
        unsafe {
            std::ptr::copy_nonoverlapping(datagram.as_ptr(), fill_buf.as_mut_ptr(), copy_len);
        }
        self.buffer.consume_datagram();
        buf.set_offset(0);
        buf.set_length(copy_len);
        Poll::Ready(Ok(()))
    }
}
