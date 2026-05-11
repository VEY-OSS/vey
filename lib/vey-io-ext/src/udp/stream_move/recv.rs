/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use bytes::Bytes;
use futures_util::FutureExt;
use tokio::time::{Instant, Sleep};

use crate::{
    ArcLimitedRecvStats, DatagramLimitAction, DatagramLimiter, GlobalDatagramLimit, UdpCopyPacket,
    UdpCopyRemoteError, UdpCopyRemoteRecv,
};

pub trait UdpMoveRecv {
    type RecvError;

    fn packet_max_size(&self) -> usize;

    fn poll_recv_packet(&mut self, cx: &mut Context<'_>) -> Poll<Result<Bytes, Self::RecvError>>;

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
        packets: &mut Vec<Bytes>,
        max_count: usize,
    ) -> Poll<Result<usize, Self::RecvError>>;
}

pub struct UdpMoveRemoteReceiver<T> {
    packet_max_size: usize,
    packets: Vec<UdpCopyPacket>,
    inner: T,
}

impl<T> UdpMoveRemoteReceiver<T> {
    pub fn new(inner: T, packet_max_size: usize) -> Self {
        UdpMoveRemoteReceiver {
            packet_max_size,
            packets: Vec::new(),
            inner,
        }
    }
}

impl<T: UdpCopyRemoteRecv> UdpMoveRecv for UdpMoveRemoteReceiver<T> {
    type RecvError = UdpCopyRemoteError;

    fn packet_max_size(&self) -> usize {
        self.packet_max_size
    }

    fn poll_recv_packet(&mut self, cx: &mut Context<'_>) -> Poll<Result<Bytes, Self::RecvError>> {
        let mut packet = self.packets.pop().unwrap_or(UdpCopyPacket::new(
            self.inner.max_hdr_len(),
            self.packet_max_size,
        ));
        match self.inner.poll_recv_packet(cx, &mut packet) {
            Poll::Ready(Ok(_)) => Poll::Ready(Ok(packet.into_payload())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => {
                self.packets.push(packet);
                Poll::Pending
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
        packets: &mut Vec<Bytes>,
        max_count: usize,
    ) -> Poll<Result<usize, Self::RecvError>> {
        self.packets.resize_with(max_count, || {
            UdpCopyPacket::new(self.inner.max_hdr_len(), self.packet_max_size)
        });

        match self.inner.poll_recv_packets(cx, &mut self.packets) {
            Poll::Ready(Ok(0)) => Poll::Ready(Ok(0)),
            Poll::Ready(Ok(n)) => {
                packets.extend(self.packets.drain(..n).map(|v| v.into_payload()));
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct LimitedUdpMoveRecv<T> {
    inner: T,
    delay: Pin<Box<Sleep>>,
    started: Instant,
    limit: DatagramLimiter,
    stats: ArcLimitedRecvStats,
}

impl<T: UdpMoveRecv> LimitedUdpMoveRecv<T> {
    pub fn local_limited(
        inner: T,
        shift_millis: u8,
        max_packets: usize,
        max_bytes: usize,
        stats: ArcLimitedRecvStats,
    ) -> Self {
        LimitedUdpMoveRecv {
            inner,
            delay: Box::pin(tokio::time::sleep(Duration::from_millis(0))),
            started: Instant::now(),
            limit: DatagramLimiter::with_local(shift_millis, max_packets, max_bytes),
            stats,
        }
    }

    #[inline]
    pub fn add_global_limiter<L>(&mut self, limiter: Arc<L>)
    where
        L: GlobalDatagramLimit + Send + Sync + 'static,
    {
        self.limit.add_global(limiter);
    }

    pub fn inner(&self) -> &T {
        &self.inner
    }

    pub fn reset_stats(&mut self, stats: ArcLimitedRecvStats) {
        self.stats = stats;
    }
}

impl<T: UdpMoveRecv> UdpMoveRecv for LimitedUdpMoveRecv<T> {
    type RecvError = T::RecvError;

    fn packet_max_size(&self) -> usize {
        self.inner.packet_max_size()
    }

    fn poll_recv_packet(&mut self, cx: &mut Context<'_>) -> Poll<Result<Bytes, Self::RecvError>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self
                .limit
                .check_packet(dur_millis, self.inner.packet_max_size())
            {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recv_packet(cx) {
                    Poll::Ready(Ok(packet)) => {
                        let nr = packet.len();
                        self.limit.set_advance(1, nr);
                        self.stats.add_recv_packet();
                        self.stats.add_recv_bytes(nr);
                        Poll::Ready(Ok(packet))
                    }
                    Poll::Ready(Err(e)) => {
                        self.limit.release_global();
                        Poll::Ready(Err(e))
                    }
                    Poll::Pending => {
                        self.limit.release_global();
                        Poll::Pending
                    }
                },
                DatagramLimitAction::DelayUntil(t) => {
                    self.delay.as_mut().reset(t);
                    match self.delay.poll_unpin(cx) {
                        Poll::Ready(_) => {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
                DatagramLimitAction::DelayFor(ms) => {
                    self.delay
                        .as_mut()
                        .reset(self.started + Duration::from_millis(dur_millis + ms));
                    match self.delay.poll_unpin(cx) {
                        Poll::Ready(_) => {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
            }
        } else {
            let packet = ready!(self.inner.poll_recv_packet(cx))?;
            self.stats.add_recv_packet();
            self.stats.add_recv_bytes(packet.len());
            Poll::Ready(Ok(packet))
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
        packets: &mut Vec<Bytes>,
        max_count: usize,
    ) -> Poll<Result<usize, Self::RecvError>> {
        use smallvec::SmallVec;

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(max_count);
            let mut total_size = 0usize;
            for _ in 0..max_count {
                total_size += self.packet_max_size();
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_recv_packets(cx, packets, n) {
                        Poll::Ready(Ok(0)) => {
                            self.limit.set_advance(0, 0);
                            Poll::Ready(Ok(0))
                        }
                        Poll::Ready(Ok(nr)) => {
                            let start = packets.len() - nr;
                            let len = packets[start..].iter().map(|h| h.len()).sum();
                            self.limit.set_advance(nr, len);
                            self.stats.add_recv_packets(nr);
                            self.stats.add_recv_bytes(len);
                            Poll::Ready(Ok(nr))
                        }
                        Poll::Ready(Err(e)) => {
                            self.limit.release_global();
                            Poll::Ready(Err(e))
                        }
                        Poll::Pending => {
                            self.limit.release_global();
                            Poll::Pending
                        }
                    }
                }
                DatagramLimitAction::DelayUntil(t) => {
                    self.delay.as_mut().reset(t);
                    match self.delay.poll_unpin(cx) {
                        Poll::Ready(_) => {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
                DatagramLimitAction::DelayFor(ms) => {
                    self.delay
                        .as_mut()
                        .reset(self.started + Duration::from_millis(dur_millis + ms));
                    match self.delay.poll_unpin(cx) {
                        Poll::Ready(_) => {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                        Poll::Pending => Poll::Pending,
                    }
                }
            }
        } else {
            let received = ready!(self.inner.poll_recv_packets(cx, packets, max_count))?;
            if received > 0 {
                self.stats.add_recv_packets(received);
                let start = packets.len() - received;
                let len = packets[start..].iter().map(|h| h.len()).sum();
                self.stats.add_recv_bytes(len);
            }
            Poll::Ready(Ok(received))
        }
    }
}
