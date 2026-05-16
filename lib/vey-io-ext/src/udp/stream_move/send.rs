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

use crate::udp::UdpCopyRemoteSend;
use crate::{
    ArcLimitedSendStats, DatagramLimitAction, DatagramLimiter, GlobalDatagramLimit,
    UdpCopyRemoteError,
};

pub trait UdpMoveSend {
    type SendError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut Option<Bytes>,
    ) -> Poll<Result<usize, Self::SendError>>;

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
        packets: &mut Vec<Bytes>,
    ) -> Poll<Result<usize, Self::SendError>>;
}

pub struct UdpMoveRemoteSender<T> {
    inner: T,
}

impl<T> UdpMoveRemoteSender<T> {
    pub fn new(inner: T) -> Self {
        UdpMoveRemoteSender { inner }
    }

    #[inline]
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T: UdpCopyRemoteSend> UdpMoveSend for UdpMoveRemoteSender<T> {
    type SendError = UdpCopyRemoteError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut Option<Bytes>,
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        if let Some(data) = &packet {
            let nw = ready!(self.inner.poll_send_buf(cx, data))?;
            if nw > 0 {
                packet.take();
            }
            Poll::Ready(Ok(nw))
        } else {
            Poll::Ready(Ok(0))
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
        cx: &mut Context<'_>,
        packets: &mut Vec<Bytes>,
    ) -> Poll<Result<usize, Self::SendError>> {
        match self.inner.poll_send_many_bytes(cx, packets.as_slice()) {
            Poll::Ready(Ok(0)) => Poll::Ready(Ok(0)),
            Poll::Ready(Ok(n)) => {
                packets.drain(..n);
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct LimitedUdpMoveSend<T> {
    inner: T,
    delay: Pin<Box<Sleep>>,
    started: Instant,
    limit: DatagramLimiter,
    stats: ArcLimitedSendStats,
}

impl<T: UdpMoveSend> LimitedUdpMoveSend<T> {
    pub fn local_limited(
        inner: T,
        shift_millis: u8,
        max_packets: usize,
        max_bytes: usize,
        stats: ArcLimitedSendStats,
    ) -> Self {
        LimitedUdpMoveSend {
            inner,
            delay: Box::pin(tokio::time::sleep(Duration::from_millis(0))),
            started: Instant::now(),
            limit: DatagramLimiter::with_local(shift_millis, max_packets, max_bytes),
            stats,
        }
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    #[inline]
    pub fn add_global_limiter<L>(&mut self, limiter: Arc<L>)
    where
        L: GlobalDatagramLimit + Send + Sync + 'static,
    {
        self.limit.add_global(limiter);
    }

    pub fn reset_stats(&mut self, stats: ArcLimitedSendStats) {
        self.stats = stats;
    }
}

impl<T: UdpMoveSend> UdpMoveSend for LimitedUdpMoveSend<T> {
    type SendError = T::SendError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut Option<Bytes>,
    ) -> Poll<Result<usize, Self::SendError>> {
        let Some(data) = &packet else {
            return Poll::Ready(Ok(0));
        };
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, data.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_send_packet(cx, packet) {
                    Poll::Ready(Ok(nw)) => {
                        if packet.is_some() {
                            self.limit.set_advance(0, 0);
                        } else {
                            self.limit.set_advance(1, nw);
                            self.stats.add_send_packet();
                            self.stats.add_send_bytes(nw);
                        }
                        Poll::Ready(Ok(nw))
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
            let nw = ready!(self.inner.poll_send_packet(cx, packet))?;
            if packet.is_none() {
                self.stats.add_send_packet();
                self.stats.add_send_bytes(nw);
            }
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
        target_os = "solaris",
    ))]
    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut Vec<Bytes>,
    ) -> Poll<Result<usize, Self::SendError>> {
        use smallvec::SmallVec;

        let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(packets.len());
        let mut total_size = 0;
        for packet in packets.iter() {
            total_size += packet.len();
            total_size_v.push(total_size);
        }

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    let not_to_send = packets.split_off(n);
                    let ret = match self.inner.poll_send_packets(cx, packets) {
                        Poll::Ready(Ok(0)) => {
                            self.limit.set_advance(0, 0);
                            Poll::Ready(Ok(0))
                        }
                        Poll::Ready(Ok(n)) => {
                            let len = total_size_v[n - 1];
                            self.limit.set_advance(n, len);
                            self.stats.add_send_packets(n);
                            self.stats.add_send_bytes(len);
                            Poll::Ready(Ok(n))
                        }
                        Poll::Ready(Err(e)) => {
                            self.limit.release_global();
                            Poll::Ready(Err(e))
                        }
                        Poll::Pending => {
                            self.limit.release_global();
                            Poll::Pending
                        }
                    };
                    packets.extend(not_to_send);
                    ret
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
            let count = ready!(self.inner.poll_send_packets(cx, packets))?;
            if count > 0 {
                self.stats.add_send_packets(count);
                self.stats.add_send_bytes(total_size_v[count - 1]);
            }
            Poll::Ready(Ok(count))
        }
    }
}
