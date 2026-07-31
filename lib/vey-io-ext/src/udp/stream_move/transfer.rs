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
                    Poll::Ready(Ok(0)) => {
                        self.recv_done = true;
                        self.active = true;
                    }
                    Poll::Ready(Ok(_)) => {
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
                        } else {
                            self.packets.push_back(packet);
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

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::task::Waker;

    use super::*;

    enum RecvStep {
        Packet(&'static [u8]),
        Eof,
        Pending,
        Error,
    }

    enum SendStep {
        Accept,
        /// the socket accepted nothing, which the move loop reports as `SendZero`
        Blocked,
        Pending,
        Error,
    }

    struct MockRecv {
        steps: VecDeque<RecvStep>,
    }

    impl MockRecv {
        fn new<I: IntoIterator<Item = RecvStep>>(steps: I) -> Self {
            MockRecv {
                steps: steps.into_iter().collect(),
            }
        }
    }

    impl UdpMoveRecv for MockRecv {
        type RecvError = &'static str;

        fn packet_max_size(&self) -> u16 {
            512
        }

        fn poll_recv_packet(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<Bytes, Self::RecvError>> {
            if matches!(self.steps.front(), Some(RecvStep::Eof)) {
                // a closed receive side stays closed
                return Poll::Ready(Ok(Bytes::new()));
            }
            match self.steps.pop_front() {
                Some(RecvStep::Packet(data)) => Poll::Ready(Ok(Bytes::from_static(data))),
                Some(RecvStep::Eof) => unreachable!("handled above"),
                Some(RecvStep::Error) => Poll::Ready(Err("mock recv failed")),
                Some(RecvStep::Pending) | None => Poll::Pending,
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
            let mut count = 0;
            while count < max_count {
                match self.poll_recv_packet(cx) {
                    Poll::Ready(Ok(packet)) => {
                        if packet.is_empty() {
                            break;
                        }
                        packets.push(packet);
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

    struct MockSend {
        steps: VecDeque<SendStep>,
        sent: Vec<Vec<u8>>,
    }

    impl MockSend {
        fn new<I: IntoIterator<Item = SendStep>>(steps: I) -> Self {
            MockSend {
                steps: steps.into_iter().collect(),
                sent: Vec::new(),
            }
        }

        fn send_one(&mut self, data: &Bytes) -> Poll<Result<usize, &'static str>> {
            match self.steps.pop_front() {
                Some(SendStep::Accept) => {
                    self.sent.push(data.to_vec());
                    Poll::Ready(Ok(data.len()))
                }
                Some(SendStep::Blocked) => Poll::Ready(Ok(0)),
                Some(SendStep::Error) => Poll::Ready(Err("mock send failed")),
                Some(SendStep::Pending) | None => Poll::Pending,
            }
        }
    }

    impl UdpMoveSend for MockSend {
        type SendError = &'static str;

        fn poll_send_packet(
            &mut self,
            _cx: &mut Context<'_>,
            packet: &mut Option<Bytes>,
        ) -> Poll<Result<usize, Self::SendError>> {
            let Some(data) = packet.as_ref() else {
                return Poll::Ready(Ok(0));
            };
            let data = data.clone();
            let nw = std::task::ready!(self.send_one(&data))?;
            if nw > 0 {
                packet.take();
            }
            Poll::Ready(Ok(nw))
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
            packets: &mut Vec<Bytes>,
        ) -> Poll<Result<usize, Self::SendError>> {
            let mut count = 0;
            for packet in packets.iter() {
                match self.send_one(packet) {
                    Poll::Ready(Ok(0)) => break,
                    Poll::Ready(Ok(_)) => count += 1,
                    Poll::Ready(Err(e)) => {
                        if count == 0 {
                            return Poll::Ready(Err(e));
                        }
                        break;
                    }
                    Poll::Pending => {
                        if count == 0 {
                            return Poll::Pending;
                        }
                        break;
                    }
                }
            }
            packets.drain(..count);
            Poll::Ready(Ok(count))
        }
    }

    fn test_config(batch_count: usize) -> LimitedUdpRelayConfig {
        let mut config = LimitedUdpRelayConfig::default();
        config.set_packet_size(512);
        config.set_batch_count(batch_count);
        config
    }

    #[tokio::test]
    async fn the_transfer_moves_packets_until_the_receiver_is_done() {
        let mut receiver = MockRecv::new([
            RecvStep::Packet(b"one"),
            RecvStep::Packet(b"two"),
            RecvStep::Eof,
        ]);
        let mut sender = MockSend::new([SendStep::Accept, SendStep::Accept]);

        let total = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(4))
            .await
            .map_err(|_| "transfer failed")
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(sender.sent, vec![b"one".to_vec(), b"two".to_vec()]);
    }

    #[tokio::test]
    async fn a_transfer_larger_than_one_batch_needs_several_rounds() {
        let mut recv_steps: Vec<RecvStep> = (0..5).map(|_| RecvStep::Packet(b"payload")).collect();
        recv_steps.push(RecvStep::Eof);
        let mut receiver = MockRecv::new(recv_steps);
        let mut sender = MockSend::new((0..5).map(|_| SendStep::Accept));

        let total = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(2))
            .await
            .map_err(|_| "transfer failed")
            .unwrap();
        assert_eq!(total, 5);
        assert_eq!(sender.sent.len(), 5);
    }

    #[tokio::test]
    async fn a_send_that_accepts_nothing_ends_the_transfer_with_send_zero() {
        let mut receiver = MockRecv::new([RecvStep::Packet(b"one"), RecvStep::Eof]);
        let mut sender = MockSend::new([SendStep::Blocked]);

        let e = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(2))
            .await
            .expect_err("the transfer should fail");
        assert!(matches!(e, UdpMoveError::SendZero));
    }

    #[tokio::test]
    async fn a_recv_failure_is_reported_as_a_recv_error() {
        let mut receiver = MockRecv::new([RecvStep::Error]);
        let mut sender = MockSend::new([]);

        let e = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(2))
            .await
            .expect_err("the transfer should fail");
        match e {
            UdpMoveError::RecvError(e) => assert_eq!(e, "mock recv failed"),
            _ => panic!("expected a recv error"),
        }
    }

    #[tokio::test]
    async fn a_send_failure_is_reported_as_a_send_error() {
        let mut receiver = MockRecv::new([RecvStep::Packet(b"one"), RecvStep::Eof]);
        let mut sender = MockSend::new([SendStep::Error]);

        let e = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(2))
            .await
            .expect_err("the transfer should fail");
        match e {
            UdpMoveError::SendError(e) => assert_eq!(e, "mock send failed"),
            _ => panic!("expected a send error"),
        }
    }

    #[tokio::test]
    async fn a_transfer_without_traffic_stays_idle() {
        let mut receiver = MockRecv::new([RecvStep::Pending]);
        let mut sender = MockSend::new([]);
        let mut transfer = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(2));
        assert!(transfer.is_idle());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut transfer).poll(&mut cx).is_pending());
        assert!(transfer.is_idle());
    }

    #[tokio::test]
    async fn a_transfer_with_traffic_becomes_active_until_it_is_reset() {
        let mut receiver = MockRecv::new([RecvStep::Packet(b"one"), RecvStep::Pending]);
        let mut sender = MockSend::new([SendStep::Accept]);
        let mut transfer = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(2));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut transfer).poll(&mut cx).is_pending());
        assert!(!transfer.is_idle());

        transfer.reset_active();
        assert!(transfer.is_idle());
    }

    #[tokio::test]
    async fn a_blocked_send_side_keeps_the_received_packets_buffered() {
        let mut receiver = MockRecv::new([
            RecvStep::Packet(b"one"),
            RecvStep::Packet(b"two"),
            RecvStep::Eof,
        ]);
        let mut sender = MockSend::new([SendStep::Pending, SendStep::Accept, SendStep::Accept]);
        let mut transfer = UdpMoveTransfer::new(&mut receiver, &mut sender, test_config(4));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut transfer).poll(&mut cx).is_pending());
        let total = match Pin::new(&mut transfer).poll(&mut cx) {
            Poll::Ready(r) => r.map_err(|_| "transfer failed").unwrap(),
            Poll::Pending => panic!("the transfer should finish once the send side is ready"),
        };

        assert_eq!(total, 2);
        assert_eq!(sender.sent, vec![b"one".to_vec(), b"two".to_vec()]);
    }
}
