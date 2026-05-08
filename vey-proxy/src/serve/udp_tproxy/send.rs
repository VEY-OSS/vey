/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::task::{Context, Poll};

use bytes::Bytes;

use vey_daemon::listen::AcceptedUdpPacketSender;
use vey_io_ext::{UdpCopyClientError, UdpCopyClientSend, UdpCopyPacket};

pub(super) struct UdpTProxyClientSend {
    inner: AcceptedUdpPacketSender,
}

impl UdpTProxyClientSend {
    pub(super) fn new(packet_sender: AcceptedUdpPacketSender) -> Self {
        UdpTProxyClientSend {
            inner: packet_sender,
        }
    }
}

impl UdpCopyClientSend for UdpTProxyClientSend {
    fn poll_send_packet(
        &mut self,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        match self.inner.send_packet(Bytes::copy_from_slice(buf)) {
            Ok(_) => Poll::Ready(Ok(buf.len())),
            Err(e) => Poll::Ready(Err(UdpCopyClientError::SendFailed(e))),
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
    fn poll_send_packets(
        &mut self,
        _cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        let mut sent = 0;
        for packet in packets {
            self.inner
                .send_packet(Bytes::copy_from_slice(packet.payload()))
                .map_err(UdpCopyClientError::SendFailed)?;
            sent += 1;
        }
        Poll::Ready(Ok(sent))
    }
}
