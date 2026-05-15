/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
)))]
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;

use super::{UdpMoveRecv, UdpMoveSend};
use crate::LimitedUdpRelayConfig;

pub enum UdpMoveError<R, S>
where
    R: UdpMoveRecv + ?Sized,
    S: UdpMoveSend + ?Sized,
{
    RecvError(R::RecvError),
    SendError(S::SendError),
    SendZero,
}

struct UdpMoveBuffer {
    config: LimitedUdpRelayConfig,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    packets: Vec<Bytes>,
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    )))]
    packets: VecDeque<Bytes>,
    recv_done: bool,
    total: u64,
    active: bool,
}

impl UdpMoveBuffer {
    fn is_idle(&self) -> bool {
        !self.active
    }

    fn reset_active(&mut self) {
        self.active = false;
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
impl UdpMoveBuffer {
    fn new(config: LimitedUdpRelayConfig) -> Self {
        let packets = Vec::with_capacity(config.batch_count);
        UdpMoveBuffer {
            config,
            packets,
            recv_done: false,
            total: 0,
            active: false,
        }
    }

    fn poll_batch_move<R, S>(
        &mut self,
        cx: &mut Context<'_>,
        receiver: &mut R,
        sender: &mut S,
    ) -> Poll<Result<u64, UdpMoveError<R, S>>>
    where
        R: UdpMoveRecv + ?Sized,
        S: UdpMoveSend + ?Sized,
    {
        let mut copy_this_round = 0usize;
        loop {
            if !self.recv_done && self.packets.len() < self.packets.capacity() {
                let max_recv = self.packets.capacity() - self.packets.len();
                match receiver.poll_recv_packets(cx, &mut self.packets, max_recv) {
                    Poll::Ready(Ok(count)) => {
                        if count == 0 {
                            self.recv_done = true;
                        }
                        self.active = true;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(UdpMoveError::RecvError(e))),
                    Poll::Pending => {
                        if self.packets.is_empty() {
                            return Poll::Pending;
                        }
                    }
                }
            }

            while !self.packets.is_empty() {
                let count = std::task::ready!(sender.poll_send_packets(cx, &mut self.packets))
                    .map_err(UdpMoveError::SendError)?;
                if count == 0 {
                    return Poll::Ready(Err(UdpMoveError::SendZero));
                }
                copy_this_round += count;
                self.total += count as u64;
                self.active = true;
            }

            if copy_this_round >= self.config.yield_count {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }

            if self.recv_done {
                return Poll::Ready(Ok(self.total));
            }
        }
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
)))]
impl UdpMoveBuffer {
    fn new(config: LimitedUdpRelayConfig) -> Self {
        let packets = VecDeque::with_capacity(config.batch_count);
        UdpMoveBuffer {
            config,
            packets,
            recv_done: false,
            total: 0,
            active: false,
        }
    }

    fn poll_batch_move<R, S>(
        &mut self,
        cx: &mut Context<'_>,
        receiver: &mut R,
        sender: &mut S,
    ) -> Poll<Result<u64, UdpMoveError<R, S>>>
    where
        R: UdpMoveRecv + ?Sized,
        S: UdpMoveSend + ?Sized,
    {
        let mut copy_this_round = 0usize;
        loop {
            if !self.recv_done && self.packets.len() < self.packets.capacity() {
                match receiver.poll_recv_packet(cx) {
                    Poll::Ready(Ok(packet)) => {
                        if packet.is_empty() {
                            self.recv_done = true;
                        }
                        self.packets.push_back(packet);
                        self.active = true;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(UdpMoveError::RecvError(e))),
                    Poll::Pending => {
                        if self.packets.is_empty() {
                            return Poll::Pending;
                        }
                    }
                }
            }

            while let Some(packet) = self.packets.pop_front() {
                let mut to_sent = Some(packet);
                match sender.poll_send_packet(cx, &mut to_sent) {
                    Poll::Ready(Ok(_)) => {
                        if let Some(packet) = to_sent {
                            self.packets.push_front(packet);
                            return Poll::Ready(Err(UdpMoveError::SendZero));
                        }
                        copy_this_round += 1;
                        self.total += 1;
                        self.active = true;
                    }
                    Poll::Ready(Err(e)) => {
                        if let Some(packet) = to_sent {
                            self.packets.push_front(packet);
                        }
                        return Poll::Ready(Err(UdpMoveError::SendError(e)));
                    }
                    Poll::Pending => {
                        if let Some(packet) = to_sent {
                            self.packets.push_front(packet);
                        }
                        return Poll::Pending;
                    }
                }
            }

            if copy_this_round >= self.config.yield_count {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }

            if self.recv_done {
                return Poll::Ready(Ok(self.total));
            }
        }
    }
}

pub struct UdpMoveTransfer<'a, R: ?Sized, S: ?Sized> {
    receiver: &'a mut R,
    sender: &'a mut S,
    buffer: UdpMoveBuffer,
}

impl<'a, R, S> UdpMoveTransfer<'a, R, S>
where
    R: UdpMoveRecv + ?Sized,
    S: UdpMoveSend + ?Sized,
{
    pub fn new(receiver: &'a mut R, sender: &'a mut S, config: LimitedUdpRelayConfig) -> Self {
        UdpMoveTransfer {
            receiver,
            sender,
            buffer: UdpMoveBuffer::new(config),
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

impl<R, S> Future for UdpMoveTransfer<'_, R, S>
where
    R: UdpMoveRecv + Unpin + ?Sized,
    S: UdpMoveSend + Unpin + ?Sized,
{
    type Output = Result<u64, UdpMoveError<R, S>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_move(cx, &mut *me.receiver, &mut *me.sender)
    }
}
