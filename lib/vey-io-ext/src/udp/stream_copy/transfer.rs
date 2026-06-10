/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll, ready};

use super::{
    UdpCopyClientError, UdpCopyClientRecv, UdpCopyClientSend, UdpCopyPacket, UdpCopyRemoteError,
    UdpCopyRemoteRecv, UdpCopyRemoteSend,
};
use crate::udp::LimitedUdpRelayConfig;

pub enum UdpCopyError<R, S> {
    RecvError(R),
    SendError(S),
    SendZero,
}

trait UdpCopyRecv {
    type Error;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), Self::Error>>;

    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpCopyPacket],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut count = 0;
        for packet in packets.iter_mut() {
            match self.poll_recv_packet(cx, packet) {
                Poll::Pending => {
                    return if count > 0 {
                        Poll::Ready(Ok(count))
                    } else {
                        Poll::Pending
                    };
                }
                Poll::Ready(Ok(_)) => {
                    if packet.payload().is_empty() {
                        break;
                    }
                    count += 1
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(count))
    }
}

struct ClientRecv<'a, T: UdpCopyClientRecv + ?Sized>(&'a mut T);

impl<T: UdpCopyClientRecv + ?Sized> UdpCopyRecv for ClientRecv<'_, T> {
    type Error = UdpCopyClientError;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), Self::Error>> {
        let (off, nr) = ready!(self.0.poll_recv_buf(cx, &mut packet.buf))?;
        packet.buf_data_off = off;
        packet.buf_data_end = nr;
        Poll::Ready(Ok(()))
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
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_recv_packets(cx, packets)
    }
}

struct RemoteRecv<'a, T: UdpCopyRemoteRecv + ?Sized>(&'a mut T);

impl<T: UdpCopyRemoteRecv + ?Sized> UdpCopyRecv for RemoteRecv<'_, T> {
    type Error = UdpCopyRemoteError;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), Self::Error>> {
        let (off, nr) = ready!(self.0.poll_recv_buf(cx, &mut packet.buf))?;
        packet.buf_data_off = off;
        packet.buf_data_end = nr;
        Poll::Ready(Ok(()))
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
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_recv_packets(cx, packets)
    }
}

trait UdpCopySend {
    type Error;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpCopyPacket,
    ) -> Poll<Result<usize, Self::Error>>;

    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut count = 0;
        for packet in packets {
            match self.poll_send_packet(cx, packet) {
                Poll::Pending => {
                    return if count > 0 {
                        Poll::Ready(Ok(count))
                    } else {
                        Poll::Pending
                    };
                }
                Poll::Ready(Ok(0)) => break,
                Poll::Ready(Ok(_)) => count += 1,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(count))
    }
}

struct ClientSend<'a, T: UdpCopyClientSend + ?Sized>(&'a mut T);

impl<T: UdpCopyClientSend + ?Sized> UdpCopySend for ClientSend<'_, T> {
    type Error = UdpCopyClientError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpCopyPacket,
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_send_buf(cx, packet.payload())
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
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_send_packets(cx, packets)
    }
}

struct RemoteSend<'a, T: UdpCopyRemoteSend + ?Sized>(&'a mut T);

impl<T: UdpCopyRemoteSend + ?Sized> UdpCopySend for RemoteSend<'_, T> {
    type Error = UdpCopyRemoteError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpCopyPacket,
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_send_buf(cx, packet.payload())
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
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.0.poll_send_many_packets(cx, packets)
    }
}

struct UdpCopyBuffer {
    config: LimitedUdpRelayConfig,
    packets: Vec<UdpCopyPacket>,
    send_start: usize,
    send_end: usize,
    recv_done: bool,
    total: u64,
    active: bool,
}

impl UdpCopyBuffer {
    fn new(max_hdr_size: usize, config: LimitedUdpRelayConfig) -> Self {
        let packets =
            vec![UdpCopyPacket::new(max_hdr_size, config.packet_size); config.batch_count];
        UdpCopyBuffer {
            config,
            packets,
            send_start: 0,
            send_end: 0,
            recv_done: false,
            total: 0,
            active: false,
        }
    }

    #[allow(clippy::type_complexity)]
    fn poll_batch_copy<R, S>(
        &mut self,
        cx: &mut Context<'_>,
        mut receiver: R,
        mut sender: S,
    ) -> Poll<Result<u64, UdpCopyError<R::Error, S::Error>>>
    where
        R: UdpCopyRecv,
        S: UdpCopySend,
    {
        let mut copy_this_round = 0usize;
        loop {
            if !self.recv_done && self.send_end < self.packets.len() {
                match receiver.poll_recv_packets(cx, &mut self.packets[self.send_end..]) {
                    Poll::Ready(Ok(0)) => {
                        self.recv_done = true;
                        self.active = true;
                    }
                    Poll::Ready(Ok(count)) => {
                        self.send_end += count;
                        self.active = true;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(UdpCopyError::RecvError(e))),
                    Poll::Pending => {
                        if self.send_start >= self.send_end {
                            return Poll::Pending;
                        }
                    }
                }
            }

            while self.send_end > self.send_start {
                let packets = &self.packets[self.send_start..self.send_end];
                let count = ready!(sender.poll_send_packets(cx, packets))
                    .map_err(UdpCopyError::SendError)?;
                if count == 0 {
                    return Poll::Ready(Err(UdpCopyError::SendZero));
                }
                copy_this_round += count;
                self.send_start += count;
                self.total += count as u64;
                self.active = true;
            }
            self.send_start = 0;
            self.send_end = 0;

            if copy_this_round >= self.config.yield_count {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }

            if self.recv_done {
                return Poll::Ready(Ok(self.total));
            }
        }
    }

    fn is_idle(&self) -> bool {
        !self.active
    }

    fn reset_active(&mut self) {
        self.active = false;
    }
}

pub struct UdpCopyClientToRemote<'a, C: ?Sized, R: ?Sized> {
    client: &'a mut C,
    remote: &'a mut R,
    buffer: UdpCopyBuffer,
}

impl<'a, C, R> UdpCopyClientToRemote<'a, C, R>
where
    C: UdpCopyClientRecv + ?Sized,
    R: UdpCopyRemoteSend + ?Sized,
{
    pub fn new(client: &'a mut C, remote: &'a mut R, config: LimitedUdpRelayConfig) -> Self {
        let buffer = UdpCopyBuffer::new(client.max_hdr_len(), config);
        UdpCopyClientToRemote {
            client,
            remote,
            buffer,
        }
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        self.buffer.is_idle()
    }

    #[inline]
    pub fn reset_active(&mut self) {
        self.buffer.reset_active()
    }
}

impl<C, R> Future for UdpCopyClientToRemote<'_, C, R>
where
    C: UdpCopyClientRecv + Unpin + ?Sized,
    R: UdpCopyRemoteSend + Unpin + ?Sized,
{
    type Output = Result<u64, UdpCopyError<UdpCopyClientError, UdpCopyRemoteError>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_copy(cx, ClientRecv(me.client), RemoteSend(me.remote))
    }
}

pub struct UdpCopyRemoteToClient<'a, C: ?Sized, R: ?Sized> {
    client: &'a mut C,
    remote: &'a mut R,
    buffer: UdpCopyBuffer,
}

impl<'a, C, R> UdpCopyRemoteToClient<'a, C, R>
where
    C: UdpCopyClientSend + ?Sized,
    R: UdpCopyRemoteRecv + ?Sized,
{
    pub fn new(client: &'a mut C, remote: &'a mut R, config: LimitedUdpRelayConfig) -> Self {
        let buffer = UdpCopyBuffer::new(remote.max_hdr_len(), config);
        UdpCopyRemoteToClient {
            client,
            remote,
            buffer,
        }
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        self.buffer.is_idle()
    }

    #[inline]
    pub fn reset_active(&mut self) {
        self.buffer.reset_active()
    }
}

impl<C, R> Future for UdpCopyRemoteToClient<'_, C, R>
where
    C: UdpCopyClientSend + Unpin + ?Sized,
    R: UdpCopyRemoteRecv + Unpin + ?Sized,
{
    type Output = Result<u64, UdpCopyError<UdpCopyRemoteError, UdpCopyClientError>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_copy(cx, RemoteRecv(&mut *me.remote), ClientSend(&mut *me.client))
    }
}
