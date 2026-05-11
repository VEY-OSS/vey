/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use std::io::IoSliceMut;
use std::task::{Context, Poll};

use vey_daemon::listen::AcceptedUdpPacketReceiver;
use vey_io_ext::{UdpCopyClientError, UdpCopyClientRecv};
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use vey_io_ext::{UdpCopyPacket, UdpCopyPacketMeta};

pub(crate) struct UdpStreamClientRecv {
    inner: AcceptedUdpPacketReceiver,
}

impl UdpStreamClientRecv {
    pub(crate) fn new(inner: AcceptedUdpPacketReceiver) -> Self {
        UdpStreamClientRecv { inner }
    }
}

impl UdpCopyClientRecv for UdpStreamClientRecv {
    fn max_hdr_len(&self) -> usize {
        0
    }

    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyClientError>> {
        match self.inner.poll_recv_packet(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(e)) => Poll::Ready(Err(UdpCopyClientError::RecvFailed(e))),
            Poll::Ready(Ok(packet)) => {
                if packet.len() > buf.len() {
                    return Poll::Ready(Err(UdpCopyClientError::InvalidPacket(
                        "packet too large".to_string(),
                    )));
                }
                buf[..packet.len()].copy_from_slice(&packet);
                Poll::Ready(Ok((0, packet.len())))
            }
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
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        let mut count = 0;
        for packet in packets.iter_mut() {
            match self.poll_recv_buf(cx, packet.buf_mut()) {
                Poll::Ready(Ok((off, len))) => {
                    let iov = IoSliceMut::new(packet.buf_mut());
                    UdpCopyPacketMeta::new(&iov, off, len).set_packet(packet);
                    count += 1;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {
                    return if count > 0 {
                        Poll::Ready(Ok(count))
                    } else {
                        Poll::Pending
                    };
                }
            }
        }
        Poll::Ready(Ok(count))
    }
}
