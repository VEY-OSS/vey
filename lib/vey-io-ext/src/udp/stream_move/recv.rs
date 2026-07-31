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

    fn packet_max_size(&self) -> u16;

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
    packet_max_size: u16,
    packets: Vec<UdpCopyPacket>,
    inner: T,
}

impl<T> UdpMoveRemoteReceiver<T> {
    pub fn new(inner: T, packet_max_size: u16) -> Self {
        UdpMoveRemoteReceiver {
            packet_max_size,
            packets: Vec::new(),
            inner,
        }
    }

    #[inline]
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

impl<T: UdpCopyRemoteRecv> UdpMoveRecv for UdpMoveRemoteReceiver<T> {
    type RecvError = UdpCopyRemoteError;

    fn packet_max_size(&self) -> u16 {
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

    fn packet_max_size(&self) -> u16 {
        self.inner.packet_max_size()
    }

    fn poll_recv_packet(&mut self, cx: &mut Context<'_>) -> Poll<Result<Bytes, Self::RecvError>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self
                .limit
                .check_packet(dur_millis, self.inner.packet_max_size() as usize)
            {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recv_packet(cx) {
                    Poll::Ready(Ok(packet)) => {
                        let nr = packet.len();
                        if nr > 0 {
                            self.limit.set_advance(1, nr);
                            self.stats.add_recv_packet();
                            self.stats.add_recv_bytes(nr);
                        } else {
                            self.limit.release_global();
                        }
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
            if !packet.is_empty() {
                self.stats.add_recv_packet();
                self.stats.add_recv_bytes(packet.len());
            }
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
                total_size += self.packet_max_size() as usize;
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_recv_packets(cx, packets, n) {
                        Poll::Ready(Ok(0)) => {
                            self.limit.release_global();
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
            let nr = ready!(self.inner.poll_recv_packets(cx, packets, max_count))?;
            if nr > 0 {
                self.stats.add_recv_packets(nr);
                let start = packets.len() - nr;
                let len = packets[start..].iter().map(|h| h.len()).sum();
                self.stats.add_recv_bytes(len);
            }
            Poll::Ready(Ok(nr))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Waker;

    use super::*;
    use crate::LimitedRecvStats;

    #[derive(Default)]
    struct TestStats {
        packets: AtomicUsize,
        bytes: AtomicUsize,
    }

    impl TestStats {
        fn packets(&self) -> usize {
            self.packets.load(Ordering::Relaxed)
        }

        fn bytes(&self) -> usize {
            self.bytes.load(Ordering::Relaxed)
        }
    }

    impl LimitedRecvStats for TestStats {
        fn add_recv_bytes(&self, size: usize) {
            self.bytes.fetch_add(size, Ordering::Relaxed);
        }

        fn add_recv_packets(&self, n: usize) {
            self.packets.fetch_add(n, Ordering::Relaxed);
        }
    }

    enum Step {
        /// a payload placed after 2 reserved header bytes
        Packet(&'static [u8]),
        Eof,
        Pending,
        Error,
    }

    struct MockRemoteRecv {
        steps: VecDeque<Step>,
    }

    impl MockRemoteRecv {
        fn new<I: IntoIterator<Item = Step>>(steps: I) -> Self {
            MockRemoteRecv {
                steps: steps.into_iter().collect(),
            }
        }
    }

    impl UdpCopyRemoteRecv for MockRemoteRecv {
        #[cfg(feature = "log")]
        fn error_logger(&self) -> Option<&slog::Logger> {
            None
        }

        fn max_hdr_len(&self) -> usize {
            2
        }

        fn poll_recv_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<Result<(usize, usize), UdpCopyRemoteError>> {
            if matches!(self.steps.front(), Some(Step::Eof)) {
                return Poll::Ready(Ok((0, 0)));
            }
            match self.steps.pop_front() {
                Some(Step::Packet(data)) => {
                    buf[2..2 + data.len()].copy_from_slice(data);
                    Poll::Ready(Ok((2, 2 + data.len())))
                }
                Some(Step::Eof) => unreachable!("handled above"),
                Some(Step::Error) => Poll::Ready(Err(UdpCopyRemoteError::RecvFailed(
                    io::Error::other("mock recv failed"),
                ))),
                Some(Step::Pending) | None => Poll::Pending,
            }
        }
    }

    #[tokio::test]
    async fn the_receiver_hands_out_the_payload_without_the_header() {
        let mut receiver =
            UdpMoveRemoteReceiver::new(MockRemoteRecv::new([Step::Packet(b"hello")]), 512);
        assert_eq!(receiver.packet_max_size(), 512);

        let mut cx = Context::from_waker(Waker::noop());
        let packet = match receiver.poll_recv_packet(&mut cx) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(packet.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn a_closed_receive_side_hands_out_an_empty_packet() {
        let mut receiver = UdpMoveRemoteReceiver::new(MockRemoteRecv::new([Step::Eof]), 512);

        let mut cx = Context::from_waker(Waker::noop());
        let packet = match receiver.poll_recv_packet(&mut cx) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert!(packet.is_empty());
    }

    #[tokio::test]
    async fn a_pending_receive_keeps_the_buffer_for_the_next_try() {
        let mut receiver = UdpMoveRemoteReceiver::new(
            MockRemoteRecv::new([Step::Pending, Step::Packet(b"later")]),
            512,
        );

        let mut cx = Context::from_waker(Waker::noop());
        assert!(receiver.poll_recv_packet(&mut cx).is_pending());
        let packet = match receiver.poll_recv_packet(&mut cx) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(packet.as_ref(), b"later");
    }

    #[tokio::test]
    async fn a_recv_error_is_forwarded() {
        let mut receiver = UdpMoveRemoteReceiver::new(MockRemoteRecv::new([Step::Error]), 512);

        let mut cx = Context::from_waker(Waker::noop());
        let e = match receiver.poll_recv_packet(&mut cx) {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert!(e.to_string().starts_with("recv failed"));
    }

    #[tokio::test]
    async fn the_limited_receiver_counts_what_it_hands_out() {
        let stats = Arc::new(TestStats::default());
        let receiver =
            UdpMoveRemoteReceiver::new(MockRemoteRecv::new([Step::Packet(b"hello")]), 512);
        let mut receiver = LimitedUdpMoveRecv::local_limited(receiver, 10, 8, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(receiver.poll_recv_packet(&mut cx).is_ready());
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);
    }

    #[tokio::test]
    async fn the_limited_receiver_does_not_count_an_empty_packet() {
        let stats = Arc::new(TestStats::default());
        let receiver = UdpMoveRemoteReceiver::new(MockRemoteRecv::new([Step::Eof]), 512);
        let mut receiver = LimitedUdpMoveRecv::local_limited(receiver, 10, 8, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(receiver.poll_recv_packet(&mut cx).is_ready());
        assert_eq!(stats.packets(), 0);
        assert_eq!(stats.bytes(), 0);
    }

    #[tokio::test]
    async fn the_limited_receiver_delays_once_the_packet_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let receiver = UdpMoveRemoteReceiver::new(
            MockRemoteRecv::new([Step::Packet(b"first"), Step::Packet(b"second")]),
            512,
        );
        let mut receiver = LimitedUdpMoveRecv::local_limited(receiver, 10, 1, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(receiver.poll_recv_packet(&mut cx).is_ready());
        assert!(receiver.poll_recv_packet(&mut cx).is_pending());
        assert_eq!(stats.packets(), 1);
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
    #[tokio::test]
    async fn the_receiver_fills_a_batch_up_to_the_requested_count() {
        let mut receiver = UdpMoveRemoteReceiver::new(
            MockRemoteRecv::new([Step::Packet(b"aa"), Step::Packet(b"bbb"), Step::Eof]),
            512,
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut packets = Vec::with_capacity(4);
        let count = match receiver.poll_recv_packets(&mut cx, &mut packets, 4) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(packets[0].as_ref(), b"aa");
        assert_eq!(packets[1].as_ref(), b"bbb");

        // the closed receive side is reported as an empty batch
        let count = match receiver.poll_recv_packets(&mut cx, &mut packets, 4) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 0);
        assert_eq!(packets.len(), 2);
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
    #[tokio::test]
    async fn the_limited_receiver_counts_a_whole_batch() {
        let stats = Arc::new(TestStats::default());
        let receiver = UdpMoveRemoteReceiver::new(
            MockRemoteRecv::new([Step::Packet(b"aa"), Step::Packet(b"bbb"), Step::Eof]),
            512,
        );
        let mut receiver = LimitedUdpMoveRecv::local_limited(receiver, 10, 8, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packets = Vec::with_capacity(4);
        let count = match receiver.poll_recv_packets(&mut cx, &mut packets, 4) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 5);
    }
}
