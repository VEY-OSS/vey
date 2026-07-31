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
                            self.limit.release_global();
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Waker;

    use super::*;
    use crate::LimitedSendStats;
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

    struct MockRemoteSend {
        steps: VecDeque<Step>,
        sent: Vec<Vec<u8>>,
    }

    impl MockRemoteSend {
        fn new<I: IntoIterator<Item = Step>>(steps: I) -> Self {
            MockRemoteSend {
                steps: steps.into_iter().collect(),
                sent: Vec::new(),
            }
        }

        fn accept(&mut self, buf: &[u8]) -> Poll<Result<usize, UdpCopyRemoteError>> {
            match self.steps.pop_front() {
                Some(Step::Accept) => {
                    self.sent.push(buf.to_vec());
                    Poll::Ready(Ok(buf.len()))
                }
                Some(Step::Blocked) => Poll::Ready(Ok(0)),
                Some(Step::Error) => Poll::Ready(Err(UdpCopyRemoteError::SendFailed(
                    io::Error::other("mock send failed"),
                ))),
                Some(Step::Pending) | None => Poll::Pending,
            }
        }
    }

    impl UdpCopyRemoteSend for MockRemoteSend {
        #[cfg(feature = "log")]
        fn error_logger(&self) -> Option<&slog::Logger> {
            None
        }

        fn poll_send_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, UdpCopyRemoteError>> {
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
        fn poll_send_many_packets(
            &mut self,
            cx: &mut Context<'_>,
            packets: &[UdpCopyPacket],
        ) -> Poll<Result<usize, UdpCopyRemoteError>> {
            let payloads: Vec<Bytes> = packets
                .iter()
                .map(|p| Bytes::copy_from_slice(p.payload()))
                .collect();
            self.poll_send_many_bytes(cx, &payloads)
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
        fn poll_send_many_bytes(
            &mut self,
            _cx: &mut Context<'_>,
            packets: &[Bytes],
        ) -> Poll<Result<usize, UdpCopyRemoteError>> {
            let mut count = 0;
            for packet in packets {
                match self.accept(packet) {
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
    async fn an_accepted_packet_is_taken_from_the_caller() {
        let mut sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Accept]));

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = Some(Bytes::from_static(b"hello"));
        assert!(matches!(
            sender.poll_send_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(5))
        ));
        assert!(packet.is_none());
        assert_eq!(sender.inner().sent, vec![b"hello".to_vec()]);
    }

    #[tokio::test]
    async fn a_blocked_socket_leaves_the_packet_with_the_caller() {
        let mut sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Blocked]));

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = Some(Bytes::from_static(b"hello"));
        assert!(matches!(
            sender.poll_send_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(packet.as_deref(), Some(&b"hello"[..]));
    }

    #[tokio::test]
    async fn an_already_sent_packet_needs_no_socket_call() {
        let mut sender = UdpMoveRemoteSender::new(MockRemoteSend::new([]));

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = None;
        assert!(matches!(
            sender.poll_send_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(0))
        ));
    }

    #[tokio::test]
    async fn a_pending_socket_leaves_the_packet_with_the_caller() {
        let mut sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Pending]));

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = Some(Bytes::from_static(b"hello"));
        assert!(sender.poll_send_packet(&mut cx, &mut packet).is_pending());
        assert_eq!(packet.as_deref(), Some(&b"hello"[..]));
    }

    #[tokio::test]
    async fn a_send_error_is_forwarded() {
        let mut sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Error]));

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = Some(Bytes::from_static(b"hello"));
        let e = match sender.poll_send_packet(&mut cx, &mut packet) {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert!(e.to_string().starts_with("send failed"));
    }

    #[tokio::test]
    async fn the_limited_sender_counts_what_was_accepted() {
        let stats = Arc::new(TestStats::default());
        let sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Accept, Step::Blocked]));
        let mut sender = LimitedUdpMoveSend::local_limited(sender, 10, 8, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = Some(Bytes::from_static(b"hello"));
        assert!(sender.poll_send_packet(&mut cx, &mut packet).is_ready());
        assert_eq!(stats.packets(), 1);
        assert_eq!(stats.bytes(), 5);

        // a packet the socket refused is not counted
        let mut packet = Some(Bytes::from_static(b"more"));
        assert!(matches!(
            sender.poll_send_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(stats.packets(), 1);
        assert!(packet.is_some());
    }

    #[tokio::test]
    async fn the_limited_sender_delays_once_the_packet_quota_is_used() {
        let stats = Arc::new(TestStats::default());
        let sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Accept, Step::Accept]));
        let mut sender = LimitedUdpMoveSend::local_limited(sender, 10, 1, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = Some(Bytes::from_static(b"first"));
        assert!(sender.poll_send_packet(&mut cx, &mut packet).is_ready());

        let mut packet = Some(Bytes::from_static(b"second"));
        assert!(sender.poll_send_packet(&mut cx, &mut packet).is_pending());
        assert!(packet.is_some());
        assert_eq!(stats.packets(), 1);
    }

    #[tokio::test]
    async fn the_limited_sender_skips_a_packet_that_is_already_gone() {
        let stats = Arc::new(TestStats::default());
        let sender = UdpMoveRemoteSender::new(MockRemoteSend::new([]));
        let mut sender = LimitedUdpMoveSend::local_limited(sender, 10, 1, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packet = None;
        assert!(matches!(
            sender.poll_send_packet(&mut cx, &mut packet),
            Poll::Ready(Ok(0))
        ));
        assert_eq!(stats.packets(), 0);
        assert!(sender.inner_mut().inner().steps.is_empty());
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
    async fn a_batch_send_drains_only_the_accepted_prefix() {
        let mut sender = UdpMoveRemoteSender::new(MockRemoteSend::new([
            Step::Accept,
            Step::Accept,
            Step::Blocked,
        ]));

        let mut cx = Context::from_waker(Waker::noop());
        let mut packets = vec![
            Bytes::from_static(b"aa"),
            Bytes::from_static(b"bb"),
            Bytes::from_static(b"cc"),
        ];
        let count = match sender.poll_send_packets(&mut cx, &mut packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(packets, vec![Bytes::from_static(b"cc")]);
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
    async fn a_limited_batch_send_only_offers_the_allowed_prefix() {
        let stats = Arc::new(TestStats::default());
        let sender = UdpMoveRemoteSender::new(MockRemoteSend::new([Step::Accept, Step::Accept]));
        let mut sender = LimitedUdpMoveSend::local_limited(sender, 10, 2, 0, stats.clone());

        let mut cx = Context::from_waker(Waker::noop());
        let mut packets = vec![
            Bytes::from_static(b"aa"),
            Bytes::from_static(b"bb"),
            Bytes::from_static(b"cc"),
        ];
        let count = match sender.poll_send_packets(&mut cx, &mut packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(stats.packets(), 2);
        assert_eq!(stats.bytes(), 4);
        // the packets left over by the limit stay in the queue
        assert_eq!(packets, vec![Bytes::from_static(b"cc")]);

        assert!(sender.poll_send_packets(&mut cx, &mut packets).is_pending());
    }
}
