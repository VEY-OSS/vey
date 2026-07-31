/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::Pin;
use std::task::{Context, Poll, ready};

use super::{
    UdpCopyClientError, UdpCopyClientRecv, UdpCopyClientSend, UdpCopyPacket, UdpCopyRemoteError,
    UdpCopyRemoteRecv, UdpCopyRemoteSend,
};
use crate::udp::LimitedUdpRelayConfig;

pub enum UdpCopyError<R, S> {
    RecvError(R),
    SendError(S),
    SendZero,
}

trait UdpCopyRecv {
    type Error;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), Self::Error>>;

    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpCopyPacket],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut count = 0;
        for packet in packets.iter_mut() {
            match self.poll_recv_packet(cx, packet) {
                Poll::Pending => {
                    return if count > 0 {
                        Poll::Ready(Ok(count))
                    } else {
                        Poll::Pending
                    };
                }
                Poll::Ready(Ok(_)) => {
                    if packet.payload().is_empty() {
                        break;
                    }
                    count += 1
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(count))
    }
}

struct ClientRecv<'a, T: UdpCopyClientRecv + ?Sized>(&'a mut T);

impl<T: UdpCopyClientRecv + ?Sized> UdpCopyRecv for ClientRecv<'_, T> {
    type Error = UdpCopyClientError;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), Self::Error>> {
        let (off, nr) = ready!(self.0.poll_recv_buf(cx, &mut packet.buf))?;
        packet.buf_data_off = off;
        packet.buf_data_end = nr;
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
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_recv_packets(cx, packets)
    }
}

struct RemoteRecv<'a, T: UdpCopyRemoteRecv + ?Sized>(&'a mut T);

impl<T: UdpCopyRemoteRecv + ?Sized> UdpCopyRecv for RemoteRecv<'_, T> {
    type Error = UdpCopyRemoteError;

    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpCopyPacket,
    ) -> Poll<Result<(), Self::Error>> {
        let (off, nr) = ready!(self.0.poll_recv_buf(cx, &mut packet.buf))?;
        packet.buf_data_off = off;
        packet.buf_data_end = nr;
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
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_recv_packets(cx, packets)
    }
}

trait UdpCopySend {
    type Error;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpCopyPacket,
    ) -> Poll<Result<usize, Self::Error>>;

    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpCopyPacket],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut count = 0;
        for packet in packets {
            match self.poll_send_packet(cx, packet) {
                Poll::Pending => {
                    return if count > 0 {
                        Poll::Ready(Ok(count))
                    } else {
                        Poll::Pending
                    };
                }
                Poll::Ready(Ok(0)) => break,
                Poll::Ready(Ok(_)) => count += 1,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(count))
    }
}

struct ClientSend<'a, T: UdpCopyClientSend + ?Sized>(&'a mut T);

impl<T: UdpCopyClientSend + ?Sized> UdpCopySend for ClientSend<'_, T> {
    type Error = UdpCopyClientError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpCopyPacket,
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_send_buf(cx, packet.payload())
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
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_send_packets(cx, packets)
    }
}

struct RemoteSend<'a, T: UdpCopyRemoteSend + ?Sized>(&'a mut T);

impl<T: UdpCopyRemoteSend + ?Sized> UdpCopySend for RemoteSend<'_, T> {
    type Error = UdpCopyRemoteError;

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpCopyPacket,
    ) -> Poll<Result<usize, Self::Error>> {
        self.0.poll_send_buf(cx, packet.payload())
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
    ) -> Poll<Result<usize, UdpCopyRemoteError>> {
        self.0.poll_send_many_packets(cx, packets)
    }
}

struct UdpCopyBuffer {
    config: LimitedUdpRelayConfig,
    packets: Vec<UdpCopyPacket>,
    send_start: usize,
    send_end: usize,
    recv_done: bool,
    total: u64,
    active: bool,
}

impl UdpCopyBuffer {
    fn new(max_hdr_size: usize, config: LimitedUdpRelayConfig) -> Self {
        let packets =
            vec![UdpCopyPacket::new(max_hdr_size, config.packet_size); config.batch_count];
        UdpCopyBuffer {
            config,
            packets,
            send_start: 0,
            send_end: 0,
            recv_done: false,
            total: 0,
            active: false,
        }
    }

    #[allow(clippy::type_complexity)]
    fn poll_batch_copy<R, S>(
        &mut self,
        cx: &mut Context<'_>,
        mut receiver: R,
        mut sender: S,
    ) -> Poll<Result<u64, UdpCopyError<R::Error, S::Error>>>
    where
        R: UdpCopyRecv,
        S: UdpCopySend,
    {
        let mut copy_this_round = 0usize;
        loop {
            if !self.recv_done && self.send_end < self.packets.len() {
                match receiver.poll_recv_packets(cx, &mut self.packets[self.send_end..]) {
                    Poll::Ready(Ok(0)) => {
                        self.recv_done = true;
                        self.active = true;
                    }
                    Poll::Ready(Ok(count)) => {
                        self.send_end += count;
                        self.active = true;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(UdpCopyError::RecvError(e))),
                    Poll::Pending => {
                        if self.send_start >= self.send_end {
                            return Poll::Pending;
                        }
                    }
                }
            }

            while self.send_end > self.send_start {
                // send_start is kept on Pending and send_end only grows, so a retried
                // batch always begins with the packets the sender may have buffered,
                // as required by the cancel safety of the send traits
                let packets = &self.packets[self.send_start..self.send_end];
                let count = ready!(sender.poll_send_packets(cx, packets))
                    .map_err(UdpCopyError::SendError)?;
                if count == 0 {
                    return Poll::Ready(Err(UdpCopyError::SendZero));
                }
                copy_this_round += count;
                self.send_start += count;
                self.total += count as u64;
                self.active = true;
            }
            self.send_start = 0;
            self.send_end = 0;

            if copy_this_round >= self.config.yield_count {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }

            if self.recv_done {
                return Poll::Ready(Ok(self.total));
            }
        }
    }

    fn is_idle(&self) -> bool {
        !self.active
    }

    fn reset_active(&mut self) {
        self.active = false;
    }
}

pub struct UdpCopyClientToRemote<'a, C: ?Sized, R: ?Sized> {
    client: &'a mut C,
    remote: &'a mut R,
    buffer: UdpCopyBuffer,
}

impl<'a, C, R> UdpCopyClientToRemote<'a, C, R>
where
    C: UdpCopyClientRecv + ?Sized,
    R: UdpCopyRemoteSend + ?Sized,
{
    pub fn new(client: &'a mut C, remote: &'a mut R, config: LimitedUdpRelayConfig) -> Self {
        let buffer = UdpCopyBuffer::new(client.max_hdr_len(), config);
        UdpCopyClientToRemote {
            client,
            remote,
            buffer,
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

impl<C, R> Future for UdpCopyClientToRemote<'_, C, R>
where
    C: UdpCopyClientRecv + Unpin + ?Sized,
    R: UdpCopyRemoteSend + Unpin + ?Sized,
{
    type Output = Result<u64, UdpCopyError<UdpCopyClientError, UdpCopyRemoteError>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_copy(cx, ClientRecv(me.client), RemoteSend(me.remote))
    }
}

pub struct UdpCopyRemoteToClient<'a, C: ?Sized, R: ?Sized> {
    client: &'a mut C,
    remote: &'a mut R,
    buffer: UdpCopyBuffer,
}

impl<'a, C, R> UdpCopyRemoteToClient<'a, C, R>
where
    C: UdpCopyClientSend + ?Sized,
    R: UdpCopyRemoteRecv + ?Sized,
{
    pub fn new(client: &'a mut C, remote: &'a mut R, config: LimitedUdpRelayConfig) -> Self {
        let buffer = UdpCopyBuffer::new(remote.max_hdr_len(), config);
        UdpCopyRemoteToClient {
            client,
            remote,
            buffer,
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

impl<C, R> Future for UdpCopyRemoteToClient<'_, C, R>
where
    C: UdpCopyClientSend + Unpin + ?Sized,
    R: UdpCopyRemoteRecv + Unpin + ?Sized,
{
    type Output = Result<u64, UdpCopyError<UdpCopyRemoteError, UdpCopyClientError>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_copy(cx, RemoteRecv(&mut *me.remote), ClientSend(&mut *me.client))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io;
    use std::task::Waker;

    use super::*;

    enum RecvStep {
        /// a payload placed after `hdr_len` reserved bytes
        Packet(&'static [u8], usize),
        Eof,
        Pending,
        Error,
    }

    enum SendStep {
        Accept,
        /// the socket accepted nothing, which the copy loop reports as `SendZero`
        Blocked,
        Pending,
        Error,
    }

    #[derive(Default)]
    struct Mock {
        recv_steps: VecDeque<RecvStep>,
        send_steps: VecDeque<SendStep>,
        sent: Vec<Vec<u8>>,
    }

    impl Mock {
        fn recving<I: IntoIterator<Item = RecvStep>>(steps: I) -> Self {
            Mock {
                recv_steps: steps.into_iter().collect(),
                ..Default::default()
            }
        }

        fn sending<I: IntoIterator<Item = SendStep>>(steps: I) -> Self {
            Mock {
                send_steps: steps.into_iter().collect(),
                ..Default::default()
            }
        }

        fn recv_one(&mut self, buf: &mut [u8]) -> Poll<Result<(usize, usize), ()>> {
            if matches!(self.recv_steps.front(), Some(RecvStep::Eof)) {
                // a closed receive side stays closed
                return Poll::Ready(Ok((0, 0)));
            }
            match self.recv_steps.pop_front() {
                Some(RecvStep::Packet(data, hdr_len)) => {
                    buf[hdr_len..hdr_len + data.len()].copy_from_slice(data);
                    Poll::Ready(Ok((hdr_len, hdr_len + data.len())))
                }
                Some(RecvStep::Eof) => unreachable!("handled above"),
                Some(RecvStep::Error) => Poll::Ready(Err(())),
                Some(RecvStep::Pending) | None => Poll::Pending,
            }
        }

        fn send_one(&mut self, buf: &[u8]) -> Poll<Result<usize, ()>> {
            match self.send_steps.pop_front() {
                Some(SendStep::Accept) => {
                    self.sent.push(buf.to_vec());
                    Poll::Ready(Ok(buf.len()))
                }
                Some(SendStep::Blocked) => Poll::Ready(Ok(0)),
                Some(SendStep::Error) => Poll::Ready(Err(())),
                Some(SendStep::Pending) | None => Poll::Pending,
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
        fn send_batch<E: Fn() -> Er, Er>(
            &mut self,
            payloads: impl IntoIterator<Item = Vec<u8>>,
            make_error: E,
        ) -> Poll<Result<usize, Er>> {
            let mut count = 0;
            for payload in payloads {
                match self.send_one(&payload) {
                    Poll::Ready(Ok(0)) => break,
                    Poll::Ready(Ok(_)) => count += 1,
                    Poll::Ready(Err(())) => {
                        return if count > 0 {
                            Poll::Ready(Ok(count))
                        } else {
                            Poll::Ready(Err(make_error()))
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

    fn client_recv_error() -> UdpCopyClientError {
        UdpCopyClientError::RecvFailed(io::Error::other("mock client recv failed"))
    }

    fn client_send_error() -> UdpCopyClientError {
        UdpCopyClientError::SendFailed(io::Error::other("mock client send failed"))
    }

    fn remote_recv_error() -> UdpCopyRemoteError {
        UdpCopyRemoteError::RecvFailed(io::Error::other("mock remote recv failed"))
    }

    fn remote_send_error() -> UdpCopyRemoteError {
        UdpCopyRemoteError::SendFailed(io::Error::other("mock remote send failed"))
    }

    impl UdpCopyClientRecv for Mock {
        fn max_hdr_len(&self) -> usize {
            4
        }

        fn poll_recv_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<Result<(usize, usize), UdpCopyClientError>> {
            self.recv_one(buf).map_err(|_| client_recv_error())
        }
    }

    impl UdpCopyClientSend for Mock {
        fn poll_send_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, UdpCopyClientError>> {
            self.send_one(buf).map_err(|_| client_send_error())
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
            let payloads: Vec<Vec<u8>> = packets.iter().map(|p| p.payload().to_vec()).collect();
            self.send_batch(payloads, client_send_error)
        }
    }

    impl UdpCopyRemoteRecv for Mock {
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
            self.recv_one(buf).map_err(|_| remote_recv_error())
        }
    }

    impl UdpCopyRemoteSend for Mock {
        #[cfg(feature = "log")]
        fn error_logger(&self) -> Option<&slog::Logger> {
            None
        }

        fn poll_send_buf(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, UdpCopyRemoteError>> {
            self.send_one(buf).map_err(|_| remote_send_error())
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
            _cx: &mut Context<'_>,
            packets: &[UdpCopyPacket],
        ) -> Poll<Result<usize, UdpCopyRemoteError>> {
            let payloads: Vec<Vec<u8>> = packets.iter().map(|p| p.payload().to_vec()).collect();
            self.send_batch(payloads, remote_send_error)
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
            packets: &[bytes::Bytes],
        ) -> Poll<Result<usize, UdpCopyRemoteError>> {
            let payloads: Vec<Vec<u8>> = packets.iter().map(|p| p.to_vec()).collect();
            self.send_batch(payloads, remote_send_error)
        }
    }

    fn test_config(batch_count: usize) -> LimitedUdpRelayConfig {
        let mut config = LimitedUdpRelayConfig::default();
        config.set_packet_size(512);
        config.set_batch_count(batch_count);
        config
    }

    #[tokio::test]
    async fn client_to_remote_copies_until_the_client_is_done() {
        let mut client = Mock::recving([
            RecvStep::Packet(b"one", 4),
            RecvStep::Packet(b"two", 4),
            RecvStep::Eof,
        ]);
        let mut remote = Mock::sending([SendStep::Accept, SendStep::Accept]);

        let total = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(4))
            .await
            .map_err(|_| "copy failed")
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(remote.sent, vec![b"one".to_vec(), b"two".to_vec()]);
    }

    #[tokio::test]
    async fn remote_to_client_copies_until_the_remote_is_done() {
        let mut client = Mock::sending([SendStep::Accept, SendStep::Accept]);
        let mut remote = Mock::recving([
            RecvStep::Packet(b"back", 2),
            RecvStep::Packet(b"again", 2),
            RecvStep::Eof,
        ]);

        let total = UdpCopyRemoteToClient::new(&mut client, &mut remote, test_config(4))
            .await
            .map_err(|_| "copy failed")
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(client.sent, vec![b"back".to_vec(), b"again".to_vec()]);
    }

    #[tokio::test]
    async fn a_copy_larger_than_one_batch_needs_several_rounds() {
        let mut recv_steps: Vec<RecvStep> =
            (0..5).map(|_| RecvStep::Packet(b"payload", 4)).collect();
        recv_steps.push(RecvStep::Eof);
        let mut client = Mock::recving(recv_steps);
        let mut remote = Mock::sending((0..5).map(|_| SendStep::Accept));

        let total = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .map_err(|_| "copy failed")
            .unwrap();
        assert_eq!(total, 5);
        assert_eq!(remote.sent.len(), 5);
    }

    #[tokio::test]
    async fn a_send_that_accepts_nothing_ends_the_copy_with_send_zero() {
        let mut client = Mock::recving([RecvStep::Packet(b"one", 4), RecvStep::Eof]);
        let mut remote = Mock::sending([SendStep::Blocked]);

        let e = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .expect_err("the copy should fail");
        assert!(matches!(e, UdpCopyError::SendZero));
    }

    #[tokio::test]
    async fn a_recv_failure_is_reported_as_a_recv_error() {
        let mut client = Mock::recving([RecvStep::Error]);
        let mut remote = Mock::sending([]);

        let e = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .expect_err("the copy should fail");
        match e {
            UdpCopyError::RecvError(e) => {
                assert!(e.to_string().starts_with("recv failed"));
            }
            _ => panic!("expected a recv error"),
        }
    }

    #[tokio::test]
    async fn a_send_failure_is_reported_as_a_send_error() {
        let mut client = Mock::recving([RecvStep::Packet(b"one", 4), RecvStep::Eof]);
        let mut remote = Mock::sending([SendStep::Error]);

        let e = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .expect_err("the copy should fail");
        match e {
            UdpCopyError::SendError(e) => {
                assert!(e.to_string().starts_with("send failed"));
            }
            _ => panic!("expected a send error"),
        }
    }

    #[tokio::test]
    async fn a_copy_without_traffic_stays_idle() {
        let mut client = Mock::recving([RecvStep::Pending]);
        let mut remote = Mock::sending([]);
        let mut copy = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(2));
        assert!(copy.is_idle());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut copy).poll(&mut cx).is_pending());
        assert!(copy.is_idle());
    }

    #[tokio::test]
    async fn a_copy_with_traffic_becomes_active_until_it_is_reset() {
        let mut client = Mock::recving([RecvStep::Packet(b"one", 4), RecvStep::Pending]);
        let mut remote = Mock::sending([SendStep::Accept]);
        let mut copy = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(2));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut copy).poll(&mut cx).is_pending());
        assert!(!copy.is_idle());

        copy.reset_active();
        assert!(copy.is_idle());
    }

    #[tokio::test]
    async fn a_blocked_send_side_keeps_the_received_packets_buffered() {
        let mut client = Mock::recving([
            RecvStep::Packet(b"one", 4),
            RecvStep::Packet(b"two", 4),
            RecvStep::Eof,
        ]);
        let mut remote = Mock::sending([SendStep::Pending, SendStep::Accept, SendStep::Accept]);
        let mut copy = UdpCopyClientToRemote::new(&mut client, &mut remote, test_config(4));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut copy).poll(&mut cx).is_pending());
        let total = match Pin::new(&mut copy).poll(&mut cx) {
            Poll::Ready(r) => r.map_err(|_| "copy failed").unwrap(),
            Poll::Pending => panic!("the copy should finish once the send side is ready"),
        };

        assert_eq!(total, 2);
        assert_eq!(remote.sent, vec![b"one".to_vec(), b"two".to_vec()]);
    }
}
