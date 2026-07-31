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
        let mut count = 0;
        for packet in packets.iter_mut() {
            match self.poll_recv_buf(cx, packet.buf_mut()) {
                Poll::Ready(Ok((off, len))) => {
                    packet.set_offset(off);
                    packet.set_length(len);
                    if len <= off {
                        break;
                    }
                    count += 1;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {
                    return if count == 0 {
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(count))
                    };
                }
            }
        }
        Poll::Ready(Ok(count))
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
                        if pkt_size > 0 {
                            self.limit.set_advance(1, pkt_size);
                            self.stats.add_recv_packet();
                            self.stats.add_recv_bytes(pkt_size);
                        }
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
            let pkt_size = end - start;
            if pkt_size > 0 {
                self.stats.add_recv_packet();
                self.stats.add_recv_bytes(pkt_size);
            }
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
                        if pkt_size > 0 {
                            self.limit.set_advance(1, pkt_size);
                            self.stats.add_recv_packet();
                            self.stats.add_recv_bytes(pkt_size);
                        } else {
                            self.limit.release_global();
                        }
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
            let pkt_size = packet.payload_len();
            if pkt_size > 0 {
                self.stats.add_recv_packet();
                self.stats.add_recv_bytes(pkt_size);
            }
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
        /// a payload placed after 4 reserved header bytes
        Packet(&'static [u8]),
        Eof,
        Pending,
        Error,
    }

    struct MockRecv {
        steps: VecDeque<Step>,
        calls: usize,
    }

    impl MockRecv {
        fn new<I: IntoIterator<Item = Step>>(steps: I) -> Self {
            MockRecv {
                steps: steps.into_iter().collect(),
                calls: 0,
            }
        }
    }

    impl UdpCopyClientRecv for MockRecv {
        fn max_hdr_len(&self) -> usize {
            4
        }

        fn poll_recv_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<Result<(usize, usize), UdpCopyClientError>> {
            self.calls += 1;
            if matches!(self.steps.front(), Some(Step::Eof)) {
                return Poll::Ready(Ok((0, 0)));
            }
            match self.steps.pop_front() {
                Some(Step::Packet(data)) => {
                    buf[4..4 + data.len()].copy_from_slice(data);
                    Poll::Ready(Ok((4, 4 + data.len())))
                }
                Some(Step::Eof) => unreachable!("handled above"),
                Some(Step::Error) => Poll::Ready(Err(UdpCopyClientError::RecvFailed(
                    io::Error::other("mock recv failed"),
                ))),
                Some(Step::Pending) | None => Poll::Pending,
            }
        }
    }

    #[tokio::test]
    async fn the_unlimited_wrapper_counts_only_the_payload() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpCopyClientRecv::unlimited(
            MockRecv::new([Step::Packet(b"hello")]),
            stats.clone(),
        );
        assert_eq!(recv.max_hdr_len(), 4);

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = UdpCopyPacket::new(recv.max_hdr_len(), 512);
        assert!(matches!(
            recv.poll_recv_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(packet.payload(), b"hello");
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);
    }

    #[tokio::test]
    async fn an_empty_packet_is_not_counted() {
        let stats = Arc::new(TestStats::default());
        let mut recv =
            LimitedUdpCopyClientRecv::unlimited(MockRecv::new([Step::Eof]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = UdpCopyPacket::new(4, 512);
        assert!(matches!(
            recv.poll_recv_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(()))
        ));
        assert!(packet.payload().is_empty());
        assert_eq!(stats.packets(), 0);
        assert_eq!(stats.bytes(), 0);
    }

    #[tokio::test]
    async fn a_limited_recv_delays_once_the_packet_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpCopyClientRecv::local_limited(
            MockRecv::new([Step::Packet(b"first"), Step::Packet(b"second")]),
            10,
            1,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = UdpCopyPacket::new(4, 512);
        assert!(recv.poll_recv_packet(&mut cx, &mut packet).is_ready());
        assert!(recv.poll_recv_packet(&mut cx, &mut packet).is_pending());
        assert_eq!(recv.inner().calls, 1);
        assert_eq!(stats.packets(), 1);
    }

    #[tokio::test]
    async fn a_pending_socket_is_forwarded() {
        let stats = Arc::new(TestStats::default());
        let mut recv =
            LimitedUdpCopyClientRecv::unlimited(MockRecv::new([Step::Pending]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = UdpCopyPacket::new(4, 512);
        assert!(recv.poll_recv_packet(&mut cx, &mut packet).is_pending());
        assert_eq!(stats.packets(), 0);
    }

    #[tokio::test]
    async fn a_recv_error_is_forwarded() {
        let stats = Arc::new(TestStats::default());
        let mut recv =
            LimitedUdpCopyClientRecv::unlimited(MockRecv::new([Step::Error]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = UdpCopyPacket::new(4, 512);
        let e = match recv.poll_recv_packet(&mut cx, &mut packet) {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert!(e.to_string().starts_with("recv failed"));
        assert_eq!(stats.packets(), 0);
    }

    #[tokio::test]
    async fn reset_stats_switches_the_counter() {
        let first = Arc::new(TestStats::default());
        let second = Arc::new(TestStats::default());
        let mut recv = LimitedUdpCopyClientRecv::unlimited(
            MockRecv::new([Step::Packet(b"one"), Step::Packet(b"two")]),
            first.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = UdpCopyPacket::new(4, 512);
        assert!(recv.poll_recv_packet(&mut cx, &mut packet).is_ready());
        recv.reset_stats(second.clone());
        assert!(recv.poll_recv_packet(&mut cx, &mut packet).is_ready());

        assert_eq!(first.packets(), 1);
        assert_eq!(second.packets(), 1);
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
    async fn the_default_batch_recv_stops_at_an_empty_packet() {
        let mut recv = MockRecv::new([Step::Packet(b"aa"), Step::Packet(b"bbb"), Step::Eof]);
        let mut packets = vec![UdpCopyPacket::new(4, 512); 4];

        let mut cx = Context::from_waker(Waker::noop());
        let count = match recv.poll_recv_packets(&mut cx, &mut packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(packets[0].payload(), b"aa");
        assert_eq!(packets[1].payload(), b"bbb");
        assert!(packets[2].payload().is_empty());
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
    async fn a_limited_batch_recv_trims_the_batch_to_the_quota() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpCopyClientRecv::local_limited(
            MockRecv::new([
                Step::Packet(b"aa"),
                Step::Packet(b"bb"),
                Step::Packet(b"cc"),
            ]),
            10,
            2,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut packets = vec![UdpCopyPacket::new(4, 512); 3];
        let count = match recv.poll_recv_packets(&mut cx, &mut packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(recv.inner().calls, 2);
        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 4);

        assert!(recv.poll_recv_packets(&mut cx, &mut packets).is_pending());
        assert_eq!(recv.inner().calls, 2);
    }
}
