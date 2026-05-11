/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::task::{Context, Poll};

use bytes::Bytes;

use vey_daemon::listen::AcceptedUdpPacketSender;
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

pub(crate) struct UdpStreamClientSend {
    inner: AcceptedUdpPacketSender,
}

impl UdpStreamClientSend {
    pub(crate) fn new(packet_sender: AcceptedUdpPacketSender) -> Self {
        UdpStreamClientSend {
            inner: packet_sender,
        }
    }
}

impl UdpCopyClientSend for UdpStreamClientSend {
    fn poll_send_buf(
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
