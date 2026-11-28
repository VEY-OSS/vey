/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, IoSliceMut};
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
use vey_io_ext::UdpCopyPacket;
use vey_io_ext::{UdpCopyPacketMeta, UdpCopyRemoteError, UdpCopyRemoteRecv};
use vey_socks::v5::UdpInput;

pub(crate) struct StackSocks5UdpConnectRemoteRecv<T, C> {
    inner: T,
    ctl_stream: C,
    end_on_control_closed: bool,
    ignore_ctl_stream: bool,
    logger: Option<Logger>,
}

impl<T, C> StackSocks5UdpConnectRemoteRecv<T, C>
where
    T: UdpCopyRemoteRecv,
    C: AsyncRead + Unpin,
{
    pub(crate) fn new(
        recv: T,
        ctl_stream: C,
        end_on_control_closed: bool,
        logger: Option<Logger>,
    ) -> Self {
        StackSocks5UdpConnectRemoteRecv {
            inner: recv,
            ctl_stream,
            end_on_control_closed,
            ignore_ctl_stream: false,
            logger,
        }
    }

    fn check_ctl_stream(&mut self, cx: &mut Context<'_>) -> Result<(), UdpCopyRemoteError> {
        const MAX_MSG_SIZE: usize = 4;
        let mut buf = [0u8; MAX_MSG_SIZE];

        let mut read_buf = ReadBuf::new(&mut buf);
        match Pin::new(&mut self.ctl_stream).poll_read(cx, &mut read_buf) {
            Poll::Pending => Ok(()),
            Poll::Ready(Ok(_)) => match read_buf.filled().len() {
                0 => {
                    if self.end_on_control_closed {
                        Err(UdpCopyRemoteError::RemoteSessionClosed)
                    } else {
                        self.ignore_ctl_stream = true;
                        Ok(())
                    }
                }
                MAX_MSG_SIZE => Err(UdpCopyRemoteError::RemoteSessionError(io::Error::other(
                    "unexpected data received in ctl stream",
                ))),
                _ => Ok(()), // drain extra data sent by some bad implementation
            },
            Poll::Ready(Err(e)) => Err(UdpCopyRemoteError::RemoteSessionError(e)),
        }
    }
}

impl<T, C> UdpCopyRemoteRecv for StackSocks5UdpConnectRemoteRecv<T, C>
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

    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyRemoteError>> {
        if !self.ignore_ctl_stream {
            self.check_ctl_stream(cx)?;
        }

        let (off, end) = ready!(self.inner.poll_recv_buf(cx, buf))?;
        let (header_len, _upstream) = UdpInput::parse_header(&buf[off..end])
            .map_err(|e| UdpCopyRemoteError::InvalidPacket(e.to_string()))?;

        self.end_on_control_closed = true;
        Poll::Ready(Ok((off + header_len, end)))
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
        if !self.ignore_ctl_stream {
            self.check_ctl_stream(cx)?;
        }

        let count = ready!(self.inner.poll_recv_packets(cx, packets))?;

        let mut parsed = Vec::with_capacity(count);
        for packet in packets.iter().take(count) {
            let payload = packet.payload();
            let (header_len, _upstream) = UdpInput::parse_header(payload)
                .map_err(|e| UdpCopyRemoteError::InvalidPacket(e.to_string()))?;
            // SAFETY: a parsed header means `payload` is a non-empty subslice of `packet.buf()`.
            let off = packet
                .buf()
                .element_offset(unsafe { &*payload.as_ptr() })
                .ok_or(UdpCopyRemoteError::InternalServerError(
                    "socks payload is outside the packet buffer",
                ))?;
            parsed.push((off + header_len, off + payload.len()));
        }
        for ((payload_off, end), packet) in parsed.into_iter().zip(packets.iter_mut()) {
            let iov = IoSliceMut::new(&mut packet.buf_mut()[..end]);
            UdpCopyPacketMeta::new(&iov, payload_off, end).set_packet(packet);
        }

        self.end_on_control_closed = true;
        Poll::Ready(Ok(count))
    }
}
