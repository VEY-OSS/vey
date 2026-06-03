/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use futures_util::FutureExt;
use tokio::time::{Instant, Sleep};

use super::UdpCopyClientError;
use crate::{
    ArcLimitedRecvStats, DatagramLimitAction, DatagramLimiter, GlobalDatagramLimit, UdpCopyPacket,
};

pub trait UdpCopyClientRecv {
    /// reserve some space for offloading header
    fn max_hdr_len(&self) -> usize;

    /// return `(off, len)`
    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyClientError>>;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut UdpCopyPacket,
    ) -> Poll<Result<(), UdpCopyClientError>> {
        let (off, len) = ready!(self.poll_recv_buf(cx, buf.buf_mut()))?;
        buf.set_length(len);
        buf.set_offset(off);
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
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        for (n, packet) in packets.iter_mut().enumerate() {
            match self.poll_recv_buf(cx, packet.buf_mut()) {
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {
                    return if n == 0 {
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(n - 1))
                    };
                }
            }
        }
        Poll::Ready(Ok(packets.len()))
    }
}

pub struct LimitedUdpCopyClientRecv<T> {
    inner: T,
    delay: Pin<Box<Sleep>>,
    started: Instant,
    limit: DatagramLimiter,
    stats: ArcLimitedRecvStats,
}

impl<T: UdpCopyClientRecv> LimitedUdpCopyClientRecv<T> {
    pub fn unlimited(inner: T, stats: ArcLimitedRecvStats) -> Self {
        LimitedUdpCopyClientRecv {
            inner,
            delay: Box::pin(tokio::time::sleep(Duration::from_millis(0))),
            started: Instant::now(),
            limit: DatagramLimiter::default(),
            stats,
        }
    }

    pub fn local_limited(
        inner: T,
        shift_millis: u8,
        max_packets: usize,
        max_bytes: usize,
        stats: ArcLimitedRecvStats,
    ) -> Self {
        LimitedUdpCopyClientRecv {
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

impl<T> UdpCopyClientRecv for LimitedUdpCopyClientRecv<T>
where
    T: UdpCopyClientRecv + Unpin,
{
    fn max_hdr_len(&self) -> usize {
        self.inner.max_hdr_len()
    }

    fn poll_recv_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, usize), UdpCopyClientError>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, buf.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recv_buf(cx, buf) {
                    Poll::Ready(Ok((start, end))) => {
                        let pkt_size = end - start;
                        self.limit.set_advance(1, pkt_size);
                        self.stats.add_recv_packet();
                        self.stats.add_recv_bytes(pkt_size);
                        Poll::Ready(Ok((start, end)))
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
            let (start, end) = ready!(self.inner.poll_recv_buf(cx, buf))?;
            self.stats.add_recv_packet();
            self.stats.add_recv_bytes(end - start);
            Poll::Ready(Ok((start, end)))
        }
    }

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), UdpCopyClientError>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, packet.buf_len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recv_packet(cx, packet) {
                    Poll::Ready(Ok(_)) => {
                        let pkt_size = packet.payload_len();
                        self.limit.set_advance(1, pkt_size);
                        self.stats.add_recv_packet();
                        self.stats.add_recv_bytes(pkt_size);
                        Poll::Ready(Ok(()))
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
            ready!(self.inner.poll_recv_packet(cx, packet))?;
            self.stats.add_recv_packet();
            self.stats.add_recv_bytes(packet.payload_len());
            Poll::Ready(Ok(()))
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
        use smallvec::SmallVec;

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(packets.len());
            let mut total_size = 0usize;
            for packet in packets.iter() {
                total_size += packet.buf_len();
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_recv_packets(cx, &mut packets[0..n]) {
                        Poll::Ready(Ok(count)) => {
                            let len = packets.iter().take(count).map(|h| h.payload_len()).sum();
                            self.limit.set_advance(count, len);
                            self.stats.add_recv_packets(count);
                            self.stats.add_recv_bytes(len);
                            Poll::Ready(Ok(count))
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
            let count = ready!(self.inner.poll_recv_packets(cx, packets))?;
            self.stats.add_recv_packets(count);
            self.stats
                .add_recv_bytes(packets.iter().take(count).map(|h| h.payload_len()).sum());
            Poll::Ready(Ok(count))
        }
    }
}
