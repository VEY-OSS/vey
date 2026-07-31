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

use vey_io_sys::udp::RecvMsgHdr;

use crate::limit::{DatagramLimitAction, DatagramLimiter};
use crate::{ArcLimitedRecvStats, GlobalDatagramLimit};

pub trait AsyncUdpRecv {
    fn poll_recv_from(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<(usize, SocketAddr)>>;

    fn poll_recv(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>>;

    fn poll_recvmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr: &mut RecvMsgHdr<'_, C>,
    ) -> Poll<io::Result<()>>;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    fn poll_batch_recvmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr_v: &mut [RecvMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>>;
}

pub struct LimitedUdpRecv<T> {
    inner: T,
    delay: Pin<Box<Sleep>>,
    started: Instant,
    limit: DatagramLimiter,
    stats: ArcLimitedRecvStats,
}

impl<T: AsyncUdpRecv> LimitedUdpRecv<T> {
    pub fn unlimited(inner: T, stats: ArcLimitedRecvStats) -> Self {
        LimitedUdpRecv {
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
        LimitedUdpRecv {
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

impl<T> AsyncUdpRecv for LimitedUdpRecv<T>
where
    T: AsyncUdpRecv + Send,
{
    fn poll_recv_from(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<(usize, SocketAddr)>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, buf.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recv_from(cx, buf) {
                    Poll::Ready(Ok((nr, addr))) => {
                        self.limit.set_advance(1, nr);
                        self.stats.add_recv_packet();
                        self.stats.add_recv_bytes(nr);
                        Poll::Ready(Ok((nr, addr)))
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
            let (nr, addr) = ready!(self.inner.poll_recv_from(cx, buf))?;
            self.stats.add_recv_packet();
            self.stats.add_recv_bytes(nr);
            Poll::Ready(Ok((nr, addr)))
        }
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            match self.limit.check_packet(dur_millis, buf.len()) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recv(cx, buf) {
                    Poll::Ready(Ok(nr)) => {
                        self.limit.set_advance(1, nr);
                        self.stats.add_recv_packet();
                        self.stats.add_recv_bytes(nr);
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
            let nr = ready!(self.inner.poll_recv(cx, buf))?;
            self.stats.add_recv_packet();
            self.stats.add_recv_bytes(nr);
            Poll::Ready(Ok(nr))
        }
    }

    fn poll_recvmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr: &mut RecvMsgHdr<'_, C>,
    ) -> Poll<io::Result<()>> {
        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let total_size = hdr.iov.iter().map(|v| v.len()).sum::<usize>();
            match self.limit.check_packet(dur_millis, total_size) {
                DatagramLimitAction::Advance(_) => match self.inner.poll_recvmsg(cx, hdr) {
                    Poll::Ready(Ok(_)) => {
                        self.limit.set_advance(1, hdr.n_recv);
                        self.stats.add_recv_packet();
                        self.stats.add_recv_bytes(hdr.n_recv);
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
            ready!(self.inner.poll_recvmsg(cx, hdr))?;
            self.stats.add_recv_packet();
            self.stats.add_recv_bytes(hdr.n_recv);
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
    fn poll_batch_recvmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr_v: &mut [RecvMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>> {
        use smallvec::SmallVec;

        if self.limit.is_set() {
            let dur_millis = self.started.elapsed().as_millis() as u64;
            let mut total_size_v = SmallVec::<[usize; 32]>::with_capacity(hdr_v.len());
            let mut total_size = 0usize;
            for hdr in hdr_v.iter() {
                total_size += hdr.iov.iter().map(|v| v.len()).sum::<usize>();
                total_size_v.push(total_size);
            }
            match self.limit.check_packets(dur_millis, total_size_v.as_ref()) {
                DatagramLimitAction::Advance(n) => {
                    match self.inner.poll_batch_recvmsg(cx, &mut hdr_v[0..n]) {
                        Poll::Ready(Ok(count)) => {
                            let len = hdr_v.iter().take(count).map(|h| h.n_recv).sum();
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
            let count = ready!(self.inner.poll_batch_recvmsg(cx, hdr_v))?;
            self.stats.add_recv_packets(count);
            self.stats
                .add_recv_bytes(hdr_v.iter().take(count).map(|h| h.n_recv).sum());
            Poll::Ready(Ok(count))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::IoSliceMut;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Waker;

    use vey_types::limit::GlobalDatagramSpeedLimitConfig;

    use super::*;
    use crate::LimitedRecvStats;
    use crate::limit::GlobalDatagramLimiter;

    const PEER_ADDR: &str = "127.0.0.1:1234";

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
        Data(&'static [u8]),
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

        fn next_data(&mut self) -> Poll<io::Result<&'static [u8]>> {
            self.calls += 1;
            match self.steps.pop_front() {
                Some(Step::Data(data)) => Poll::Ready(Ok(data)),
                Some(Step::Error) => Poll::Ready(Err(io::Error::other("mock recv failed"))),
                Some(Step::Pending) | None => Poll::Pending,
            }
        }
    }

    impl AsyncUdpRecv for MockRecv {
        fn poll_recv_from(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<(usize, SocketAddr)>> {
            let data = ready!(self.next_data())?;
            buf[..data.len()].copy_from_slice(data);
            Poll::Ready(Ok((data.len(), PEER_ADDR.parse().unwrap())))
        }

        fn poll_recv(&mut self, _cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
            let data = ready!(self.next_data())?;
            buf[..data.len()].copy_from_slice(data);
            Poll::Ready(Ok(data.len()))
        }

        fn poll_recvmsg<const C: usize>(
            &mut self,
            _cx: &mut Context<'_>,
            hdr: &mut RecvMsgHdr<'_, C>,
        ) -> Poll<io::Result<()>> {
            let data = ready!(self.next_data())?;
            hdr.iov[0][..data.len()].copy_from_slice(data);
            hdr.n_recv = data.len();
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
        fn poll_batch_recvmsg<const C: usize>(
            &mut self,
            _cx: &mut Context<'_>,
            hdr_v: &mut [RecvMsgHdr<'_, C>],
        ) -> Poll<io::Result<usize>> {
            let mut count = 0;
            for hdr in hdr_v.iter_mut() {
                match self.next_data() {
                    Poll::Ready(Ok(data)) => {
                        hdr.iov[0][..data.len()].copy_from_slice(data);
                        hdr.n_recv = data.len();
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
    }

    #[tokio::test]
    async fn unlimited_recv_from_records_every_packet() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::unlimited(
            MockRecv::new([Step::Data(b"hello"), Step::Data(b"bye")]),
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];

        let (nr, addr) = match recv.poll_recv_from(&mut cx, &mut buf) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(&buf[..nr], b"hello");
        assert_eq!(addr, PEER_ADDR.parse::<SocketAddr>().unwrap());

        let nr = match recv.poll_recv(&mut cx, &mut buf) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(&buf[..nr], b"bye");

        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 8);
    }

    #[tokio::test]
    async fn unlimited_recv_error_is_not_counted() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::unlimited(MockRecv::new([Step::Error]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];
        let e = match recv.poll_recv(&mut cx, &mut buf) {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(e.to_string(), "mock recv failed");
        assert_eq!(stats.packets(), 0);
        assert_eq!(stats.bytes(), 0);
    }

    #[tokio::test]
    async fn unlimited_recvmsg_counts_the_received_length() {
        let stats = Arc::new(TestStats::default());
        let mut recv =
            LimitedUdpRecv::unlimited(MockRecv::new([Step::Data(b"abcd")]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];
        let mut hdr = RecvMsgHdr::new([IoSliceMut::new(&mut buf)]);
        assert!(matches!(
            recv.poll_recvmsg(&mut cx, &mut hdr),
            Poll::Ready(Ok(()))
        ));
        assert_eq!(hdr.n_recv, 4);
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 4);
    }

    #[tokio::test]
    async fn inner_pending_is_forwarded() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::unlimited(MockRecv::new([Step::Pending]), stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];
        assert!(recv.poll_recv(&mut cx, &mut buf).is_pending());
        assert_eq!(stats.packets(), 0);
    }

    #[tokio::test]
    async fn local_limited_recv_delays_once_the_packet_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::local_limited(
            MockRecv::new([Step::Data(b"first"), Step::Data(b"second")]),
            10,
            1,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];
        assert!(matches!(
            recv.poll_recv(&mut cx, &mut buf),
            Poll::Ready(Ok(5))
        ));
        // the second packet is over the per-window quota, so the inner socket is not touched
        assert!(recv.poll_recv(&mut cx, &mut buf).is_pending());
        assert_eq!(recv.inner().calls, 1);
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);
    }

    #[tokio::test]
    async fn local_limited_recv_delays_once_the_byte_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::local_limited(
            MockRecv::new([Step::Data(b"0123456789"), Step::Data(b"more")]),
            10,
            0,
            12,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];
        assert!(matches!(
            recv.poll_recv(&mut cx, &mut buf),
            Poll::Ready(Ok(10))
        ));
        assert!(recv.poll_recv(&mut cx, &mut buf).is_pending());
        assert_eq!(recv.inner().calls, 1);
        assert_eq!(stats.bytes(), 10);
    }

    #[tokio::test]
    async fn global_limiter_makes_an_unlimited_recv_limited() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::unlimited(
            MockRecv::new([Step::Data(b"abcd"), Step::Data(b"efgh")]),
            stats.clone(),
        );
        recv.add_global_limiter(Arc::new(GlobalDatagramLimiter::new(
            GlobalDatagramSpeedLimitConfig::per_second(16),
        )));

        let mut cx = Context::from_waker(Waker::noop());
        // buf.len() is what the limiter reserves, so a 16 byte buffer drains the budget
        let mut buf = [0u8; 16];
        assert!(matches!(
            recv.poll_recv(&mut cx, &mut buf),
            Poll::Ready(Ok(4))
        ));
        assert!(recv.poll_recv(&mut cx, &mut buf).is_pending());
        assert_eq!(recv.inner().calls, 1);
        assert_eq!(stats.packets(), 1);
    }

    #[tokio::test]
    async fn reset_stats_switches_the_counter() {
        let first = Arc::new(TestStats::default());
        let second = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::unlimited(
            MockRecv::new([Step::Data(b"one"), Step::Data(b"two")]),
            first.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut buf = [0u8; 16];
        assert!(recv.poll_recv(&mut cx, &mut buf).is_ready());
        recv.reset_stats(second.clone());
        assert!(recv.poll_recv(&mut cx, &mut buf).is_ready());

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
    async fn unlimited_batch_recvmsg_counts_the_whole_batch() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::unlimited(
            MockRecv::new([Step::Data(b"aa"), Step::Data(b"bbb"), Step::Pending]),
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut b1 = [0u8; 8];
        let mut b2 = [0u8; 8];
        let mut b3 = [0u8; 8];
        let mut hdr_v = [
            RecvMsgHdr::new([IoSliceMut::new(&mut b1)]),
            RecvMsgHdr::new([IoSliceMut::new(&mut b2)]),
            RecvMsgHdr::new([IoSliceMut::new(&mut b3)]),
        ];

        let count = match recv.poll_batch_recvmsg(&mut cx, &mut hdr_v) {
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
        target_os = "macos",
        target_os = "solaris",
    ))]
    #[tokio::test]
    async fn local_limited_batch_recvmsg_trims_the_batch_to_the_quota() {
        let stats = Arc::new(TestStats::default());
        let mut recv = LimitedUdpRecv::local_limited(
            MockRecv::new([Step::Data(b"aa"), Step::Data(b"bb"), Step::Data(b"cc")]),
            10,
            2,
            0,
            stats.clone(),
        );

        let mut cx = Context::from_waker(Waker::noop());
        let mut b1 = [0u8; 8];
        let mut b2 = [0u8; 8];
        let mut b3 = [0u8; 8];
        let mut hdr_v = [
            RecvMsgHdr::new([IoSliceMut::new(&mut b1)]),
            RecvMsgHdr::new([IoSliceMut::new(&mut b2)]),
            RecvMsgHdr::new([IoSliceMut::new(&mut b3)]),
        ];

        let count = match recv.poll_batch_recvmsg(&mut cx, &mut hdr_v) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(recv.inner().calls, 2);
        assert_eq!(stats.packets(), 2);

        // the quota is used up, so the next batch must not reach the inner socket
        assert!(recv.poll_batch_recvmsg(&mut cx, &mut hdr_v).is_pending());
        assert_eq!(recv.inner().calls, 2);
    }
}
