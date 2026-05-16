/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
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

pub(crate) struct DirectUdpConnectRemoteSend<T> {
    inner: T,
    logger: Option<Logger>,
}

impl<T> DirectUdpConnectRemoteSend<T>
where
    T: AsyncUdpSend,
{
    pub(crate) fn new(send: T, logger: Option<Logger>) -> Self {
        DirectUdpConnectRemoteSend {
            inner: send,
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
        use std::io::IoSlice;
        use vey_io_sys::udp::SendMsgHdr;

        let mut msgs: Vec<SendMsgHdr<1>> = packets
            .iter()
            .map(|p| SendMsgHdr::new([IoSlice::new(p.as_payload())], None))
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
        use std::io::IoSlice;
        use vey_io_sys::udp::SendMsgHdr;

        let mut msgs: Vec<SendMsgHdr<1>> = packets
            .iter()
            .map(|p| SendMsgHdr::new([IoSlice::new(p.as_payload())], None))
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

impl<T> UdpCopyRemoteSend for DirectUdpConnectRemoteSend<T>
where
    T: AsyncUdpSend,
{
    fn error_logger(&self) -> Option<&Logger> {
        self.logger.as_ref()
    }

    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        let nw = ready!(self.inner.poll_send(cx, buf)).map_err(UdpCopyRemoteError::SendFailed)?;
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
