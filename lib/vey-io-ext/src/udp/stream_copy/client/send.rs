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
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use crate::UdpCopyPacket;
use crate::{ArcLimitedSendStats, DatagramLimitAction, DatagramLimiter, GlobalDatagramLimit};

pub trait UdpCopyClientSend {
    /// return `nw`, which should be greater than 0
    ///
    /// Not cancel safe: the implementation may have buffered `buf` before returning
    /// `Pending`, so the caller has to retry with the same packet, or the buffered one
    /// will be sent in place of the new one.
    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyClientError>>;

    /// return the count of accepted packets, which may be less than `packets.len()`
    ///
    /// Not cancel safe: the implementation may have buffered some packets before
    /// returning `Pending`, so the caller has to retry with a batch that still begins
    /// with those packets in the same order, and may only append new ones to the tail.
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
    ) -> Poll<Result<usize, UdpCopyClientError>>;
}

pub struct LimitedUdpCopyClientSend<T> {
    inner: T,
    delay: Pin<Box<Sleep>>,
    started: Instant,
    limit: DatagramLimiter,
    stats: ArcLimitedSendStats,
}

impl<T: UdpCopyClientSend> LimitedUdpCopyClientSend<T> {
    pub fn unlimited(inner: T, stats: ArcLimitedSendStats) -> Self {
        LimitedUdpCopyClientSend {
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
        stats: ArcLimitedSendStats,
    ) -> Self {
        LimitedUdpCopyClientSend {
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

    pub fn reset_stats(&mut self, stats: ArcLimitedSendStats) {
        self.stats = stats;
    }
}

impl<T> UdpCopyClientSend for LimitedUdpCopyClientSend<T>
where
    T: UdpCopyClientSend + Unpin,
{
    fn poll_send_buf(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, buf.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_send_buf(cx, buf) {
                    Poll::Ready(Ok(0)) => {
                        self.limit.release_global();
                        Poll::Ready(Ok(0))
                    }
                    Poll::Ready(Ok(nw)) => {
                        self.limit.set_advance(1, nw);
                        self.stats.add_send_packet();
                        self.stats.add_send_bytes(nw);
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
            let nw = ready!(self.inner.poll_send_buf(cx, buf))?;
            if nw > 0 {
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
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, UdpCopyClientError>> {
        use smallvec::SmallVec;

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(packets.len());
            let mut total_size = 0;
            for packet in packets.iter() {
                total_size += packet.payload_len();
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_send_packets(cx, &packets[0..n]) {
                        Poll::Ready(Ok(0)) => {
                            self.limit.release_global();
                            Poll::Ready(Ok(0))
                        }
                        Poll::Ready(Ok(count)) => {
                            let len = total_size_v[count - 1];
                            self.limit.set_advance(count, len);
                            self.stats.add_send_packets(count);
                            self.stats.add_send_bytes(len);
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
            let count = ready!(self.inner.poll_send_packets(cx, packets))?;
            self.stats.add_send_packets(count);
            self.stats
                .add_send_bytes(packets.iter().take(count).map(|h| h.payload_len()).sum());
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
    use crate::LimitedSendStats;

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

    impl LimitedSendStats for TestStats {
        fn add_send_bytes(&self, size: usize) {
            self.bytes.fetch_add(size, Ordering::Relaxed);
        }

        fn add_send_packets(&self, n: usize) {
            self.packets.fetch_add(n, Ordering::Relaxed);
        }
    }

    enum Step {
        Accept,
        /// the socket accepted nothing
        Blocked,
        Pending,
        Error,
    }

    struct MockSend {
        steps: VecDeque<Step>,
    }

    impl MockSend {
        fn new<I: IntoIterator<Item = Step>>(steps: I) -> Self {
            MockSend {
                steps: steps.into_iter().collect(),
            }
        }

        fn accept(&mut self, buf: &[u8]) -> Poll<Result<usize, UdpCopyClientError>> {
            match self.steps.pop_front() {
                Some(Step::Accept) => Poll::Ready(Ok(buf.len())),
                Some(Step::Blocked) => Poll::Ready(Ok(0)),
                Some(Step::Error) => Poll::Ready(Err(UdpCopyClientError::SendFailed(
                    io::Error::other("mock send failed"),
                ))),
                Some(Step::Pending) | None => Poll::Pending,
            }
        }
    }

    impl UdpCopyClientSend for MockSend {
        fn poll_send_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, UdpCopyClientError>> {
            self.accept(buf)
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
            let mut count = 0;
            for packet in packets {
                match self.accept(packet.payload()) {
                    Poll::Ready(Ok(0)) => break,
                    Poll::Ready(Ok(_)) => count += 1,
                    Poll::Ready(Err(e)) => {
                        return if count > 0 {
                            Poll::Ready(Ok(count))
                        } else {
                            Poll::Ready(Err(e))
                        };
                    }
                    Poll::Pending => {
                        return if count > 0 {
                            Poll::Ready(Ok(count))
                        } else {
                            Poll::Pending
                        };
                    }
                }
            }
            Poll::Ready(Ok(count))
        }
    }

    #[tokio::test]
    async fn the_unlimited_wrapper_counts_what_was_sent() {
        let stats = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpCopyClientSend::unlimited(MockSend::new([Step::Accept]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            send.poll_send_buf(&mut cx, b"hello"),
            Poll::Ready(Ok(5))
        ));
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);
    }

    #[tokio::test]
    async fn a_blocked_socket_is_not_counted() {
        let stats = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpCopyClientSend::unlimited(MockSend::new([Step::Blocked]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            send.poll_send_buf(&mut cx, b"hello"),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(stats.packets(), 0);
        assert_eq!(stats.bytes(), 0);
    }

    #[tokio::test]
    async fn a_blocked_socket_under_a_limit_is_not_counted_either() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpCopyClientSend::local_limited(
            MockSend::new([Step::Blocked, Step::Accept]),
            10,
            8,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            send.poll_send_buf(&mut cx, b"hello"),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(stats.packets(), 0);
        // the rejected packet did not consume the quota
        assert!(matches!(
            send.poll_send_buf(&mut cx, b"hello"),
            Poll::Ready(Ok(5))
        ));
        assert_eq!(stats.packets(), 1);
    }

    #[tokio::test]
    async fn a_limited_send_delays_once_the_packet_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpCopyClientSend::local_limited(
            MockSend::new([Step::Accept, Step::Accept]),
            10,
            1,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        assert!(send.poll_send_buf(&mut cx, b"first").is_ready());
        assert!(send.poll_send_buf(&mut cx, b"second").is_pending());
        assert_eq!(stats.packets(), 1);
    }

    #[tokio::test]
    async fn a_pending_socket_is_forwarded() {
        let stats = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpCopyClientSend::unlimited(MockSend::new([Step::Pending]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(send.poll_send_buf(&mut cx, b"hello").is_pending());
        assert_eq!(stats.packets(), 0);
    }

    #[tokio::test]
    async fn a_send_error_is_forwarded() {
        let stats = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpCopyClientSend::unlimited(MockSend::new([Step::Error]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let e = match send.poll_send_buf(&mut cx, b"hello") {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert!(e.to_string().starts_with("send failed"));
        assert_eq!(stats.packets(), 0);
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
    async fn a_limited_batch_send_counts_only_the_accepted_prefix() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpCopyClientSend::local_limited(
            MockSend::new([Step::Accept, Step::Accept, Step::Blocked]),
            10,
            8,
            0,
            stats.clone(),
        );

        let mut packets = vec![UdpCopyPacket::new(0, 512); 3];
        for (i, packet) in packets.iter_mut().enumerate() {
            packet.buf_mut()[..2].copy_from_slice(&[b'a' + i as u8; 2]);
            packet.set_offset(0);
            packet.set_length(2);
        }

        let mut cx = Context::from_waker(Waker::noop());
        let count = match send.poll_send_packets(&mut cx, &packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 4);
    }
}
