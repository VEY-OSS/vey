/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, IoSlice};
use std::task::{Context, Poll, ready};

use slog::Logger;

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
))]
use vey_io_ext::{AsUdpPayload, UdpCopyPacket};
use vey_io_ext::{AsyncUdpSend, UdpCopyRemoteError, UdpCopyRemoteSend};
use vey_io_sys::udp::SendMsgHdr;
use vey_socks::v5::UdpOutput;
use vey_types::net::UpstreamAddr;

pub(crate) struct ProxySocks5UdpConnectRemoteSend<T> {
    inner: T,
    socks5_header: Vec<u8>,
    logger: Option<Logger>,
}

impl<T> ProxySocks5UdpConnectRemoteSend<T>
where
    T: AsyncUdpSend,
{
    pub(crate) fn new(send: T, upstream: &UpstreamAddr, logger: Option<Logger>) -> Self {
        let header_len = UdpOutput::calc_header_len(upstream);
        let mut socks5_header = vec![0; header_len];
        UdpOutput::generate_header(&mut socks5_header, upstream);
        ProxySocks5UdpConnectRemoteSend {
            inner: send,
            socks5_header,
            logger,
        }
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
    ))]
    fn poll_send_many<B: AsUdpPayload>(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[B],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        let mut msgs: Vec<SendMsgHdr<2>> = packets
            .iter()
            .map(|p| {
                SendMsgHdr::new(
                    [
                        IoSlice::new(&self.socks5_header),
                        IoSlice::new(p.as_payload()),
                    ],
                    None,
                )
            })
            .collect();

        let count = ready!(self.inner.poll_batch_sendmsg(cx, &mut msgs))
            .map_err(UdpCopyRemoteError::SendFailed)?;
        if count == 0 {
            Poll::Ready(Err(UdpCopyRemoteError::SendFailed(io::Error::new(
                io::ErrorKind::WriteZero,
                "write zero packet into sender",
            ))))
        } else {
            Poll::Ready(Ok(count))
        }
    }

    #[cfg(target_os = "macos")]
    fn poll_send_many<B: AsUdpPayload>(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[B],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        let mut msgs: Vec<SendMsgHdr<2>> = packets
            .iter()
            .map(|p| {
                SendMsgHdr::new(
                    [
                        IoSlice::new(&self.socks5_header),
                        IoSlice::new(p.as_payload()),
                    ],
                    None,
                )
            })
            .collect();

        let count = ready!(self.inner.poll_batch_sendmsg_x(cx, &mut msgs))
            .map_err(UdpCopyRemoteError::SendFailed)?;
        if count == 0 {
            Poll::Ready(Err(UdpCopyRemoteError::SendFailed(io::Error::new(
                io::ErrorKind::WriteZero,
                "write zero packet into sender",
            ))))
        } else {
            Poll::Ready(Ok(count))
        }
    }
}

impl<T> UdpCopyRemoteSend for ProxySocks5UdpConnectRemoteSend<T>
where
    T: AsyncUdpSend + Send,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        let hdr = SendMsgHdr::new([IoSlice::new(&self.socks5_header), IoSlice::new(buf)], None);
        let nw =
            ready!(self.inner.poll_sendmsg(cx, &hdr)).map_err(UdpCopyRemoteError::SendFailed)?;
        if nw == 0 {
            Poll::Ready(Err(UdpCopyRemoteError::SendFailed(io::Error::new(
                io::ErrorKind::WriteZero,
                "write zero byte into sender",
            ))))
        } else {
            Poll::Ready(Ok(nw))
        }
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
    ))]
    fn poll_send_many_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.poll_send_many(cx, packets)
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
    ))]
    fn poll_send_many_bytes(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[bytes::Bytes],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.poll_send_many(cx, packets)
    }
}
