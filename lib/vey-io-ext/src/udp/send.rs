/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::time::Duration;

use futures_util::FutureExt;
use tokio::time::{Instant, Sleep};

use vey_io_sys::udp::SendMsgHdr;

use crate::limit::{DatagramLimitAction, DatagramLimiter};
use crate::{ArcLimitedSendStats, GlobalDatagramLimit};

pub trait AsyncUdpSend {
    fn poll_send_to(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: SocketAddr,
    ) -> Poll<io::Result<usize>>;

    fn poll_send(&mut self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>>;

    fn poll_sendmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr: &SendMsgHdr<'_, C>,
    ) -> Poll<io::Result<usize>>;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    fn poll_batch_sendmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        msgs: &mut [SendMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>>;

    #[cfg(target_os = "macos")]
    fn poll_batch_sendmsg_x<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        msgs: &mut [SendMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>>;
}

pub struct LimitedUdpSend<T> {
    inner: T,
    delay: Pin<Box<Sleep>>,
    started: Instant,
    limit: DatagramLimiter,
    stats: ArcLimitedSendStats,
}

impl<T: AsyncUdpSend> LimitedUdpSend<T> {
    pub fn unlimited(inner: T, stats: ArcLimitedSendStats) -> LimitedUdpSend<T> {
        LimitedUdpSend {
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
        LimitedUdpSend {
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

impl<T> AsyncUdpSend for LimitedUdpSend<T>
where
    T: AsyncUdpSend + Send,
{
    fn poll_send_to(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: SocketAddr,
    ) -> Poll<io::Result<usize>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, buf.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_send_to(cx, buf, target) {
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
            let nw = ready!(self.inner.poll_send_to(cx, buf, target))?;
            self.stats.add_send_packet();
            self.stats.add_send_bytes(nw);
            Poll::Ready(Ok(nw))
        }
    }

    fn poll_send(&mut self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, buf.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_send(cx, buf) {
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
            let nw = ready!(self.inner.poll_send(cx, buf))?;
            self.stats.add_send_packet();
            self.stats.add_send_bytes(nw);
            Poll::Ready(Ok(nw))
        }
    }

    fn poll_sendmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr: &SendMsgHdr<'_, C>,
    ) -> Poll<io::Result<usize>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let len = hdr.iov.iter().map(|v| v.len()).sum();
            match self.limit.check_packet(dur_millis, len) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_sendmsg(cx, hdr) {
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
            let nw = ready!(self.inner.poll_sendmsg(cx, hdr))?;
            self.stats.add_send_packet();
            self.stats.add_send_bytes(nw);
            Poll::Ready(Ok(nw))
        }
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    fn poll_batch_sendmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        msgs: &mut [SendMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>> {
        use smallvec::SmallVec;

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(msgs.len());
            let mut total_size = 0;
            for msg in msgs.iter() {
                total_size += msg.iov.iter().map(|v| v.len()).sum::<usize>();
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_batch_sendmsg(cx, &mut msgs[0..n]) {
                        Poll::Ready(Ok(0)) => {
                            self.limit.set_advance(0, 0);
                            Poll::Ready(Ok(0))
                        }
                        Poll::Ready(Ok(count)) => {
                            let len = msgs.iter().take(count).map(|v| v.n_send).sum();
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
            let count = ready!(self.inner.poll_batch_sendmsg(cx, msgs))?;
            self.stats.add_send_packets(count);
            self.stats
                .add_send_bytes(msgs.iter().take(count).map(|h| h.n_send).sum());
            Poll::Ready(Ok(count))
        }
    }

    #[cfg(target_os = "macos")]
    fn poll_batch_sendmsg_x<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        msgs: &mut [SendMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>> {
        use smallvec::SmallVec;

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(msgs.len());
            let mut total_size = 0;
            for msg in msgs.iter() {
                total_size += msg.iov.iter().map(|v| v.len()).sum::<usize>();
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_batch_sendmsg_x(cx, &mut msgs[0..n]) {
                        Poll::Ready(Ok(0)) => {
                            self.limit.set_advance(0, 0);
                            Poll::Ready(Ok(0))
                        }
                        Poll::Ready(Ok(count)) => {
                            let len = msgs.iter().take(count).map(|v| v.n_send).sum();
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
            let count = ready!(self.inner.poll_batch_sendmsg_x(cx, msgs))?;
            self.stats.add_send_packets(count);
            self.stats
                .add_send_bytes(msgs.iter().take(count).map(|h| h.n_send).sum());
            Poll::Ready(Ok(count))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::IoSlice;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Waker;

    use vey_types::limit::GlobalDatagramSpeedLimitConfig;

    use super::*;
    use crate::LimitedSendStats;
    use crate::limit::GlobalDatagramLimiter;

    const PEER_ADDR: &str = "127.0.0.1:1234";

    fn peer() -> SocketAddr {
        PEER_ADDR.parse().unwrap()
    }

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
        Sent,
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "solaris",
        ))]
        Blocked,
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "solaris",
        ))]
        Pending,
        Error,
    }

    #[derive(Clone, Default)]
    struct SentLog(Arc<std::sync::Mutex<Vec<Vec<u8>>>>);

    impl SentLog {
        fn push(&self, buf: &[u8]) {
            self.0.lock().unwrap().push(buf.to_vec());
        }

        fn packets(&self) -> Vec<Vec<u8>> {
            self.0.lock().unwrap().clone()
        }
    }

    struct MockSend {
        steps: VecDeque<Step>,
        sent: SentLog,
    }

    fn join_iov<const C: usize>(hdr: &SendMsgHdr<'_, C>) -> Vec<u8> {
        let mut buf = Vec::new();
        for iov in hdr.iov.iter() {
            buf.extend_from_slice(iov);
        }
        buf
    }

    impl MockSend {
        fn new<I: IntoIterator<Item = Step>>(steps: I) -> Self {
            MockSend::with_log(steps, SentLog::default())
        }

        fn with_log<I: IntoIterator<Item = Step>>(steps: I, sent: SentLog) -> Self {
            MockSend {
                steps: steps.into_iter().collect(),
                sent,
            }
        }

        fn accept(&mut self, buf: &[u8]) -> Poll<io::Result<usize>> {
            match self.steps.pop_front() {
                Some(Step::Sent) => {
                    self.sent.push(buf);
                    Poll::Ready(Ok(buf.len()))
                }
                #[cfg(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                    target_os = "solaris",
                ))]
                Some(Step::Blocked) => Poll::Ready(Ok(0)),
                Some(Step::Error) => Poll::Ready(Err(io::Error::other("mock send failed"))),
                #[cfg(any(
                    target_os = "linux",
                    target_os = "android",
                    target_os = "freebsd",
                    target_os = "netbsd",
                    target_os = "openbsd",
                    target_os = "solaris",
                ))]
                Some(Step::Pending) => Poll::Pending,
                None => Poll::Pending,
            }
        }
    }

    impl AsyncUdpSend for MockSend {
        fn poll_send_to(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
            _target: SocketAddr,
        ) -> Poll<io::Result<usize>> {
            self.accept(buf)
        }

        fn poll_send(&mut self, _cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            self.accept(buf)
        }

        fn poll_sendmsg<const C: usize>(
            &mut self,
            _cx: &mut Context<'_>,
            hdr: &SendMsgHdr<'_, C>,
        ) -> Poll<io::Result<usize>> {
            let buf = join_iov(hdr);
            self.accept(&buf)
        }

        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "solaris",
        ))]
        fn poll_batch_sendmsg<const C: usize>(
            &mut self,
            _cx: &mut Context<'_>,
            msgs: &mut [SendMsgHdr<'_, C>],
        ) -> Poll<io::Result<usize>> {
            let mut count = 0;
            for msg in msgs.iter_mut() {
                let buf = join_iov(msg);
                match self.accept(&buf) {
                    Poll::Ready(Ok(0)) => break,
                    Poll::Ready(Ok(nw)) => {
                        msg.n_send = nw;
                        count += 1;
                    }
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

        #[cfg(target_os = "macos")]
        fn poll_batch_sendmsg_x<const C: usize>(
            &mut self,
            cx: &mut Context<'_>,
            msgs: &mut [SendMsgHdr<'_, C>],
        ) -> Poll<io::Result<usize>> {
            let mut count = 0;
            for msg in msgs.iter_mut() {
                let buf = join_iov(msg);
                match self.accept(&buf) {
                    Poll::Ready(Ok(0)) => break,
                    Poll::Ready(Ok(nw)) => {
                        msg.n_send = nw;
                        count += 1;
                    }
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
            let _ = cx;
            Poll::Ready(Ok(count))
        }
    }

    #[tokio::test]
    async fn unlimited_send_records_every_packet() {
        let stats = Arc::new(TestStats::default());
        let sent = SentLog::default();
        let mut send = LimitedUdpSend::unlimited(
            MockSend::with_log([Step::Sent, Step::Sent], sent.clone()),
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            send.poll_send_to(&mut cx, b"hello", peer()),
            Poll::Ready(Ok(5))
        ));
        assert!(matches!(
            send.poll_send(&mut cx, b"bye"),
            Poll::Ready(Ok(3))
        ));
        assert_eq!(sent.packets(), vec![b"hello".to_vec(), b"bye".to_vec()]);
        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 8);
    }

    #[tokio::test]
    async fn unlimited_send_error_is_not_counted() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpSend::unlimited(MockSend::new([Step::Error]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let e = match send.poll_send(&mut cx, b"hello") {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(e.to_string(), "mock send failed");
        assert_eq!(stats.packets(), 0);
    }

    #[tokio::test]
    async fn unlimited_sendmsg_joins_all_the_slices() {
        let stats = Arc::new(TestStats::default());
        let sent = SentLog::default();
        let mut send = LimitedUdpSend::unlimited(
            MockSend::with_log([Step::Sent], sent.clone()),
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let hdr = SendMsgHdr::new([IoSlice::new(b"ab"), IoSlice::new(b"cde")], Some(peer()));
        assert!(matches!(
            send.poll_sendmsg(&mut cx, &hdr),
            Poll::Ready(Ok(5))
        ));
        assert_eq!(sent.packets(), vec![b"abcde".to_vec()]);
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);
    }

    #[tokio::test]
    async fn local_limited_send_delays_once_the_packet_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpSend::local_limited(
            MockSend::new([Step::Sent, Step::Sent]),
            10,
            1,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            send.poll_send(&mut cx, b"first"),
            Poll::Ready(Ok(5))
        ));
        assert!(send.poll_send(&mut cx, b"second").is_pending());
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);
    }

    #[tokio::test]
    async fn global_limiter_makes_an_unlimited_send_limited() {
        let stats = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpSend::unlimited(MockSend::new([Step::Sent, Step::Sent]), stats.clone());
        send.add_global_limiter(Arc::new(GlobalDatagramLimiter::new(
            GlobalDatagramSpeedLimitConfig::per_second(8),
        )));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(matches!(
            send.poll_send(&mut cx, b"12345678"),
            Poll::Ready(Ok(8))
        ));
        assert!(send.poll_send(&mut cx, b"12345678").is_pending());
        assert_eq!(stats.packets(), 1);
    }

    #[tokio::test]
    async fn reset_stats_switches_the_counter() {
        let first = Arc::new(TestStats::default());
        let second = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpSend::unlimited(MockSend::new([Step::Sent, Step::Sent]), first.clone());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(send.poll_send(&mut cx, b"one").is_ready());
        send.reset_stats(second.clone());
        assert!(send.poll_send(&mut cx, b"two").is_ready());

        assert_eq!(first.packets(), 1);
        assert_eq!(second.packets(), 1);
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    #[tokio::test]
    async fn unlimited_batch_sendmsg_counts_the_whole_batch() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpSend::unlimited(
            MockSend::new([Step::Sent, Step::Sent, Step::Pending]),
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut msgs = [
            SendMsgHdr::new([IoSlice::new(b"aa")], Some(peer())),
            SendMsgHdr::new([IoSlice::new(b"bbb")], Some(peer())),
            SendMsgHdr::new([IoSlice::new(b"cccc")], Some(peer())),
        ];

        let count = match send.poll_batch_sendmsg(&mut cx, &mut msgs) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 5);
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    #[tokio::test]
    async fn limited_batch_sendmsg_reports_a_blocked_socket_without_stats() {
        let stats = Arc::new(TestStats::default());
        let mut send =
            LimitedUdpSend::local_limited(MockSend::new([Step::Blocked]), 10, 8, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut msgs = [SendMsgHdr::new([IoSlice::new(b"aa")], Some(peer()))];
        assert!(matches!(
            send.poll_batch_sendmsg(&mut cx, &mut msgs),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(stats.packets(), 0);
        assert_eq!(stats.bytes(), 0);
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    #[tokio::test]
    async fn local_limited_batch_sendmsg_trims_the_batch_to_the_quota() {
        let stats = Arc::new(TestStats::default());
        let mut send = LimitedUdpSend::local_limited(
            MockSend::new([Step::Sent, Step::Sent, Step::Sent]),
            10,
            2,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut msgs = [
            SendMsgHdr::new([IoSlice::new(b"aa")], Some(peer())),
            SendMsgHdr::new([IoSlice::new(b"bb")], Some(peer())),
            SendMsgHdr::new([IoSlice::new(b"cc")], Some(peer())),
        ];

        let count = match send.poll_batch_sendmsg(&mut cx, &mut msgs) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(stats.packets(), 2);

        assert!(send.poll_batch_sendmsg(&mut cx, &mut msgs).is_pending());
    }
}
