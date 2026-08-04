/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::IoSliceMut;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

use thiserror::Error;

use vey_types::net::UpstreamAddr;

use super::LimitedUdpRelayConfig;

mod client;
mod remote;

pub use client::{UdpRelayClientError, UdpRelayClientRecv, UdpRelayClientSend};
pub use remote::{UdpRelayRemoteError, UdpRelayRemoteRecv, UdpRelayRemoteSend};

#[derive(Clone)]
pub struct UdpRelayPacket {
    buf: Box<[u8]>,
    buf_data_off: usize,
    buf_data_end: usize,
    ups: UpstreamAddr,
}

impl UdpRelayPacket {
    fn new(reserved_size: usize, packet_size: u16) -> Self {
        let buf_size = packet_size as usize + reserved_size;
        UdpRelayPacket {
            // SAFETY: only `buf[off..end]` is read after recv fills that range.
            buf: unsafe { Box::<[u8]>::new_uninit_slice(buf_size).assume_init() },
            buf_data_off: 0,
            buf_data_end: 0,
            ups: UpstreamAddr::empty(),
        }
    }

    #[inline]
    pub fn buf_mut(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    #[inline]
    pub fn buf(&self) -> &[u8] {
        &self.buf
    }

    #[inline]
    fn set_offset(&mut self, off: usize) {
        self.buf_data_off = off;
    }

    #[inline]
    fn set_length(&mut self, len: usize) {
        self.buf_data_end = len;
    }

    #[inline]
    fn set_upstream(&mut self, ups: UpstreamAddr) {
        self.ups = ups;
    }

    #[inline]
    pub fn upstream(&self) -> &UpstreamAddr {
        &self.ups
    }

    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.buf[self.buf_data_off..self.buf_data_end]
    }
}

pub struct UdpRelayPacketMeta {
    iov_base: *const u8,
    data_off: usize,
    data_len: usize,
    ups: UpstreamAddr,
}

impl UdpRelayPacketMeta {
    pub fn new(iov: &IoSliceMut, data_off: usize, data_len: usize, ups: UpstreamAddr) -> Self {
        UdpRelayPacketMeta {
            iov_base: iov.as_ptr(),
            data_off,
            data_len,
            ups,
        }
    }

    pub fn set_packet(self, p: &mut UdpRelayPacket) {
        let iov_advance = p.buf().element_offset(unsafe { &*self.iov_base }).unwrap();
        p.set_offset(iov_advance + self.data_off);
        p.set_length(iov_advance + self.data_len);
        p.set_upstream(self.ups);
    }
}

#[derive(Error, Debug)]
pub enum UdpRelayError {
    #[error("client: {0}")]
    ClientError(#[from] UdpRelayClientError),
    #[error("remote: {1}")]
    RemoteError(Option<UpstreamAddr>, UdpRelayRemoteError),
}

trait UdpRelayRecv {
    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpRelayPacket,
    ) -> Poll<Result<usize, UdpRelayError>>;

    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &mut [UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayError>> {
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
                Poll::Ready(Ok(_)) => count += 1,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(count))
    }
}

struct ClientRecv<'a, T: UdpRelayClientRecv + ?Sized>(&'a mut T);

impl<T: UdpRelayClientRecv + ?Sized> UdpRelayRecv for ClientRecv<'_, T> {
    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpRelayPacket,
    ) -> Poll<Result<usize, UdpRelayError>> {
        let (off, nr, ups) = ready!(
            self.0
                .poll_recv_packet(cx, &mut packet.buf)
                .map_err(UdpRelayError::ClientError)
        )?;
        packet.buf_data_off = off;
        packet.buf_data_end = nr;
        packet.ups = ups;
        Poll::Ready(Ok(nr))
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
        packets: &mut [UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayError>> {
        self.0
            .poll_recv_packets(cx, packets)
            .map_err(UdpRelayError::ClientError)
    }
}

struct RemoteRecv<'a, T: UdpRelayRemoteRecv + ?Sized>(&'a mut T);

impl<T: UdpRelayRemoteRecv + ?Sized> UdpRelayRecv for RemoteRecv<'_, T> {
    fn poll_recv_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut UdpRelayPacket,
    ) -> Poll<Result<usize, UdpRelayError>> {
        let (off, nr, ups) = ready!(
            self.0
                .poll_recv_packet(cx, &mut packet.buf)
                .map_err(|e| UdpRelayError::RemoteError(None, e))
        )?;
        packet.buf_data_off = off;
        packet.buf_data_end = nr;
        packet.ups = ups;
        Poll::Ready(Ok(nr))
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
        packets: &mut [UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayError>> {
        self.0
            .poll_recv_packets(cx, packets)
            .map_err(|e| UdpRelayError::RemoteError(None, e))
    }
}

trait UdpRelaySend {
    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpRelayPacket,
    ) -> Poll<Result<usize, UdpRelayError>>;

    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayError>> {
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
                Poll::Ready(Ok(_)) => count += 1,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            }
        }
        Poll::Ready(Ok(count))
    }
}

struct ClientSend<'a, T: UdpRelayClientSend + ?Sized>(&'a mut T);

impl<T: UdpRelayClientSend + ?Sized> UdpRelaySend for ClientSend<'_, T> {
    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpRelayPacket,
    ) -> Poll<Result<usize, UdpRelayError>> {
        self.0
            .poll_send_packet(cx, packet.payload(), &packet.ups)
            .map_err(UdpRelayError::ClientError)
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
        packets: &[UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayError>> {
        self.0
            .poll_send_packets(cx, packets)
            .map_err(UdpRelayError::ClientError)
    }
}

struct RemoteSend<'a, T: UdpRelayRemoteSend + ?Sized>(&'a mut T);

impl<T: UdpRelayRemoteSend + ?Sized> UdpRelaySend for RemoteSend<'_, T> {
    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &UdpRelayPacket,
    ) -> Poll<Result<usize, UdpRelayError>> {
        self.0
            .poll_send_packet(cx, packet.payload(), &packet.ups)
            .map_err(|e| UdpRelayError::RemoteError(Some(packet.ups.clone()), e))
    }

    fn poll_send_packets(
        &mut self,
        cx: &mut Context<'_>,
        packets: &[UdpRelayPacket],
    ) -> Poll<Result<usize, UdpRelayError>> {
        self.0
            .poll_send_packets(cx, packets)
            .map_err(|e| UdpRelayError::RemoteError(None, e))
    }
}

struct UdpRelayBuffer {
    config: LimitedUdpRelayConfig,
    packets: Vec<UdpRelayPacket>,
    send_start: usize,
    send_end: usize,
    recv_done: bool,
    total: u64,
    active: bool,
}

impl UdpRelayBuffer {
    fn new(max_hdr_size: usize, config: LimitedUdpRelayConfig) -> Self {
        let packets =
            vec![UdpRelayPacket::new(max_hdr_size, config.packet_size); config.batch_count];
        UdpRelayBuffer {
            config,
            packets,
            send_start: 0,
            send_end: 0,
            recv_done: false,
            total: 0,
            active: false,
        }
    }

    fn poll_batch_relay<R, S>(
        &mut self,
        cx: &mut Context<'_>,
        mut receiver: R,
        mut sender: S,
    ) -> Poll<Result<u64, UdpRelayError>>
    where
        R: UdpRelayRecv,
        S: UdpRelaySend,
    {
        let mut copy_this_round = 0usize;
        loop {
            if !self.recv_done && self.send_end < self.packets.len() {
                match receiver.poll_recv_packets(cx, &mut self.packets[self.send_end..]) {
                    Poll::Ready(Ok(count)) => {
                        if count == 0 {
                            self.recv_done = true;
                        }
                        self.send_end += count;
                        self.active = true;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => {
                        if self.send_start >= self.send_end {
                            return Poll::Pending;
                        }
                    }
                }
            }

            while self.send_end > self.send_start {
                let packets = &self.packets[self.send_start..self.send_end];
                let count = ready!(sender.poll_send_packets(cx, packets))?;
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

pub struct UdpRelayClientToRemote<'a, C: ?Sized, R: ?Sized> {
    client: &'a mut C,
    remote: &'a mut R,
    buffer: UdpRelayBuffer,
}

impl<'a, C, R> UdpRelayClientToRemote<'a, C, R>
where
    C: UdpRelayClientRecv + ?Sized,
    R: UdpRelayRemoteSend + ?Sized,
{
    pub fn new(client: &'a mut C, remote: &'a mut R, config: LimitedUdpRelayConfig) -> Self {
        let buffer = UdpRelayBuffer::new(client.max_hdr_len(), config);
        UdpRelayClientToRemote {
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

impl<C, R> Future for UdpRelayClientToRemote<'_, C, R>
where
    C: UdpRelayClientRecv + Unpin + ?Sized,
    R: UdpRelayRemoteSend + Unpin + ?Sized,
{
    type Output = Result<u64, UdpRelayError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_relay(cx, ClientRecv(me.client), RemoteSend(me.remote))
    }
}

pub struct UdpRelayRemoteToClient<'a, C: ?Sized, R: ?Sized> {
    client: &'a mut C,
    remote: &'a mut R,
    buffer: UdpRelayBuffer,
}

impl<'a, C, R> UdpRelayRemoteToClient<'a, C, R>
where
    C: UdpRelayClientSend + ?Sized,
    R: UdpRelayRemoteRecv + ?Sized,
{
    pub fn new(client: &'a mut C, remote: &'a mut R, config: LimitedUdpRelayConfig) -> Self {
        let buffer = UdpRelayBuffer::new(remote.max_hdr_len(), config);
        UdpRelayRemoteToClient {
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

impl<C, R> Future for UdpRelayRemoteToClient<'_, C, R>
where
    C: UdpRelayClientSend + Unpin + ?Sized,
    R: UdpRelayRemoteRecv + Unpin + ?Sized,
{
    type Output = Result<u64, UdpRelayError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let me = &mut *self;
        me.buffer
            .poll_batch_relay(cx, RemoteRecv(me.remote), ClientSend(me.client))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::task::Waker;

    use super::*;

    fn ups(port: u16) -> UpstreamAddr {
        UpstreamAddr::from(SocketAddr::from_str(&format!("127.0.0.1:{port}")).unwrap())
    }

    #[test]
    fn packet_payload_skips_the_reserved_header() {
        let mut packet = UdpRelayPacket::new(4, 512);
        assert_eq!(packet.buf().len(), 516);
        assert!(packet.payload().is_empty());

        packet.buf_mut()[4..8].copy_from_slice(b"data");
        packet.set_offset(4);
        packet.set_length(8);
        packet.set_upstream(ups(1000));

        assert_eq!(packet.payload(), b"data");
        assert_eq!(packet.upstream(), &ups(1000));
    }

    #[test]
    fn packet_meta_offsets_are_relative_to_the_packet_buffer() {
        let mut packet = UdpRelayPacket::new(8, 512);
        packet.buf_mut()[8..16].copy_from_slice(b"hdrHELLO");

        let meta = {
            // the iov starts 8 bytes into the packet buffer, and the payload starts
            // 3 bytes into the iov
            let (_, tail) = packet.buf_mut().split_at_mut(8);
            let iov = IoSliceMut::new(tail);
            UdpRelayPacketMeta::new(&iov, 3, 8, ups(2000))
        };
        meta.set_packet(&mut packet);

        assert_eq!(packet.payload(), b"HELLO");
        assert_eq!(packet.upstream(), &ups(2000));
    }

    #[test]
    fn relay_error_display_tells_the_failing_side() {
        let client_err = UdpRelayError::ClientError(UdpRelayClientError::AddressNotSupported);
        assert_eq!(client_err.to_string(), "client: address not supported");

        let remote_err =
            UdpRelayError::RemoteError(Some(ups(3000)), UdpRelayRemoteError::NoListenSocket);
        assert_eq!(remote_err.to_string(), "remote: no listen socket");
    }

    #[test]
    fn the_default_batch_recv_stops_at_the_first_pending() {
        struct SeqRecv(VecDeque<Option<&'static [u8]>>);

        impl UdpRelayRecv for SeqRecv {
            fn poll_recv_packet(
                &mut self,
                _cx: &mut Context<'_>,
                packet: &mut UdpRelayPacket,
            ) -> Poll<Result<usize, UdpRelayError>> {
                match self.0.pop_front().flatten() {
                    Some(data) => {
                        packet.buf_mut()[..data.len()].copy_from_slice(data);
                        packet.set_offset(0);
                        packet.set_length(data.len());
                        Poll::Ready(Ok(data.len()))
                    }
                    None => Poll::Pending,
                }
            }
        }

        let mut recv = SeqRecv(VecDeque::from([Some(&b"aa"[..]), Some(&b"bbb"[..]), None]));
        let mut packets = vec![UdpRelayPacket::new(0, 512); 4];
        let mut cx = Context::from_waker(Waker::noop());

        let count = match UdpRelayRecv::poll_recv_packets(&mut recv, &mut cx, &mut packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(packets[0].payload(), b"aa");
        assert_eq!(packets[1].payload(), b"bbb");

        // with nothing received at all the whole batch stays pending
        assert!(UdpRelayRecv::poll_recv_packets(&mut recv, &mut cx, &mut packets).is_pending());
    }

    #[test]
    fn the_default_batch_send_stops_at_the_first_pending() {
        struct SeqSend {
            budget: usize,
            sent: Vec<Vec<u8>>,
        }

        impl UdpRelaySend for SeqSend {
            fn poll_send_packet(
                &mut self,
                _cx: &mut Context<'_>,
                packet: &UdpRelayPacket,
            ) -> Poll<Result<usize, UdpRelayError>> {
                if self.budget == 0 {
                    return Poll::Pending;
                }
                self.budget -= 1;
                self.sent.push(packet.payload().to_vec());
                Poll::Ready(Ok(packet.payload().len()))
            }
        }

        let mut packets = vec![UdpRelayPacket::new(0, 512); 3];
        for (i, packet) in packets.iter_mut().enumerate() {
            packet.buf_mut()[0] = b'a' + i as u8;
            packet.set_offset(0);
            packet.set_length(1);
        }

        let mut sender = SeqSend {
            budget: 2,
            sent: Vec::new(),
        };
        let mut cx = Context::from_waker(Waker::noop());
        let count = match UdpRelaySend::poll_send_packets(&mut sender, &mut cx, &packets) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert_eq!(count, 2);
        assert_eq!(sender.sent, vec![b"a".to_vec(), b"b".to_vec()]);
        assert!(UdpRelaySend::poll_send_packets(&mut sender, &mut cx, &packets).is_pending());
    }
}

/// The relay loop only ends on an empty read, which the client and remote traits can
/// only report through their batch receive methods.
#[cfg(test)]
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
mod batch_tests {
    use std::collections::VecDeque;
    use std::io;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::task::Waker;

    use super::*;
    use crate::udp::MINIMUM_UDP_RELAY_YIELD_COUNT;

    fn ups(port: u16) -> UpstreamAddr {
        UpstreamAddr::from(SocketAddr::from_str(&format!("127.0.0.1:{port}")).unwrap())
    }

    enum RecvStep {
        /// a payload placed after `hdr_len` reserved bytes, coming from/to the given port
        Packet(&'static [u8], usize, u16),
        Eof,
        Pending,
        Error,
    }

    enum SendStep {
        Accept,
        Pending,
        Error,
    }

    struct Mock {
        recv_steps: VecDeque<RecvStep>,
        send_steps: VecDeque<SendStep>,
        sent: Vec<(Vec<u8>, UpstreamAddr)>,
    }

    impl Mock {
        fn recving<I: IntoIterator<Item = RecvStep>>(steps: I) -> Self {
            Mock {
                recv_steps: steps.into_iter().collect(),
                send_steps: VecDeque::new(),
                sent: Vec::new(),
            }
        }

        fn sending<I: IntoIterator<Item = SendStep>>(steps: I) -> Self {
            Mock {
                recv_steps: VecDeque::new(),
                send_steps: steps.into_iter().collect(),
                sent: Vec::new(),
            }
        }

        /// fill one packet, returning whether the receive side is still open
        fn recv_one(&mut self, packet: &mut UdpRelayPacket) -> Poll<Result<bool, ()>> {
            if matches!(self.recv_steps.front(), Some(RecvStep::Eof)) {
                // a closed receive side stays closed
                return Poll::Ready(Ok(false));
            }
            match self.recv_steps.pop_front() {
                Some(RecvStep::Packet(data, hdr_len, port)) => {
                    packet.buf_mut()[hdr_len..hdr_len + data.len()].copy_from_slice(data);
                    packet.set_offset(hdr_len);
                    packet.set_length(hdr_len + data.len());
                    packet.set_upstream(ups(port));
                    Poll::Ready(Ok(true))
                }
                Some(RecvStep::Eof) => unreachable!("handled above"),
                Some(RecvStep::Error) => Poll::Ready(Err(())),
                Some(RecvStep::Pending) | None => Poll::Pending,
            }
        }

        fn recv_batch<E: FnOnce() -> Er, Er>(
            &mut self,
            packets: &mut [UdpRelayPacket],
            make_error: E,
        ) -> Poll<Result<usize, Er>> {
            let mut count = 0;
            for packet in packets.iter_mut() {
                match self.recv_one(packet) {
                    Poll::Ready(Ok(true)) => count += 1,
                    Poll::Ready(Ok(false)) => break,
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

        fn send_one(&mut self, buf: &[u8], addr: &UpstreamAddr) -> Poll<Result<usize, ()>> {
            match self.send_steps.pop_front() {
                Some(SendStep::Accept) => {
                    self.sent.push((buf.to_vec(), addr.clone()));
                    Poll::Ready(Ok(buf.len()))
                }
                Some(SendStep::Error) => Poll::Ready(Err(())),
                Some(SendStep::Pending) | None => Poll::Pending,
            }
        }
    }

    impl UdpRelayClientRecv for Mock {
        fn max_hdr_len(&self) -> usize {
            4
        }

        fn poll_recv_packet(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<Result<(usize, usize, UpstreamAddr), UdpRelayClientError>> {
            let mut packet = UdpRelayPacket::new(0, buf.len() as u16);
            match self.recv_one(&mut packet) {
                Poll::Ready(Ok(true)) => {
                    buf[..packet.buf_data_end]
                        .copy_from_slice(&packet.buf()[..packet.buf_data_end]);
                    Poll::Ready(Ok((packet.buf_data_off, packet.buf_data_end, packet.ups)))
                }
                Poll::Ready(Ok(false)) => Poll::Ready(Ok((0, 0, UpstreamAddr::empty()))),
                Poll::Ready(Err(())) => Poll::Ready(Err(UdpRelayClientError::RecvFailed(
                    io::Error::other("mock client recv failed"),
                ))),
                Poll::Pending => Poll::Pending,
            }
        }

        fn poll_recv_packets(
            &mut self,
            _cx: &mut Context<'_>,
            packets: &mut [UdpRelayPacket],
        ) -> Poll<Result<usize, UdpRelayClientError>> {
            self.recv_batch(packets, || {
                UdpRelayClientError::RecvFailed(io::Error::other("mock client recv failed"))
            })
        }
    }

    impl UdpRelayClientSend for Mock {
        fn poll_send_packet(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
            from: &UpstreamAddr,
        ) -> Poll<Result<usize, UdpRelayClientError>> {
            self.send_one(buf, from).map_err(|_| {
                UdpRelayClientError::SendFailed(io::Error::other("mock client send failed"))
            })
        }

        fn poll_send_packets(
            &mut self,
            cx: &mut Context<'_>,
            packets: &[UdpRelayPacket],
        ) -> Poll<Result<usize, UdpRelayClientError>> {
            let mut count = 0;
            for packet in packets {
                match UdpRelayClientSend::poll_send_packet(
                    self,
                    cx,
                    packet.payload(),
                    packet.upstream(),
                ) {
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

    impl UdpRelayRemoteRecv for Mock {
        #[cfg(feature = "log")]
        fn error_logger(&self) -> Option<&slog::Logger> {
            None
        }

        fn max_hdr_len(&self) -> usize {
            2
        }

        fn poll_recv_packet(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<Result<(usize, usize, UpstreamAddr), UdpRelayRemoteError>> {
            let mut packet = UdpRelayPacket::new(0, buf.len() as u16);
            match self.recv_one(&mut packet) {
                Poll::Ready(Ok(true)) => {
                    buf[..packet.buf_data_end]
                        .copy_from_slice(&packet.buf()[..packet.buf_data_end]);
                    Poll::Ready(Ok((packet.buf_data_off, packet.buf_data_end, packet.ups)))
                }
                Poll::Ready(Ok(false)) => Poll::Ready(Ok((0, 0, UpstreamAddr::empty()))),
                Poll::Ready(Err(())) => Poll::Ready(Err(UdpRelayRemoteError::InternalServerError(
                    "mock remote recv failed",
                ))),
                Poll::Pending => Poll::Pending,
            }
        }

        fn poll_recv_packets(
            &mut self,
            _cx: &mut Context<'_>,
            packets: &mut [UdpRelayPacket],
        ) -> Poll<Result<usize, UdpRelayRemoteError>> {
            self.recv_batch(packets, || {
                UdpRelayRemoteError::InternalServerError("mock remote recv failed")
            })
        }
    }

    impl UdpRelayRemoteSend for Mock {
        #[cfg(feature = "log")]
        fn error_logger(&self) -> Option<&slog::Logger> {
            None
        }

        fn poll_send_packet(
            &mut self,
            _cx: &mut Context<'_>,
            buf: &[u8],
            to: &UpstreamAddr,
        ) -> Poll<Result<usize, UdpRelayRemoteError>> {
            self.send_one(buf, to)
                .map_err(|_| UdpRelayRemoteError::AddressNotSupported)
        }
    }

    fn test_config(batch_count: usize) -> LimitedUdpRelayConfig {
        let mut config = LimitedUdpRelayConfig::default();
        config.set_packet_size(512);
        config.set_batch_count(batch_count);
        config
    }

    #[tokio::test]
    async fn client_to_remote_relays_until_the_client_is_done() {
        let mut client = Mock::recving([
            RecvStep::Packet(b"one", 4, 1001),
            RecvStep::Packet(b"two", 4, 1002),
            RecvStep::Eof,
        ]);
        let mut remote = Mock::sending([SendStep::Accept, SendStep::Accept]);

        let total = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(4))
            .await
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(
            remote.sent,
            vec![(b"one".to_vec(), ups(1001)), (b"two".to_vec(), ups(1002))]
        );
    }

    #[tokio::test]
    async fn remote_to_client_relays_until_the_remote_is_done() {
        let mut client = Mock::sending([SendStep::Accept, SendStep::Accept]);
        let mut remote = Mock::recving([
            RecvStep::Packet(b"back", 2, 2001),
            RecvStep::Packet(b"again", 2, 2002),
            RecvStep::Eof,
        ]);

        let total = UdpRelayRemoteToClient::new(&mut client, &mut remote, test_config(4))
            .await
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(
            client.sent,
            vec![
                (b"back".to_vec(), ups(2001)),
                (b"again".to_vec(), ups(2002))
            ]
        );
    }

    #[tokio::test]
    async fn a_relay_larger_than_one_batch_needs_several_rounds() {
        let mut recv_steps: Vec<RecvStep> = (0..5)
            .map(|_| RecvStep::Packet(b"payload", 4, 1001))
            .collect();
        recv_steps.push(RecvStep::Eof);
        let mut client = Mock::recving(recv_steps);
        let mut remote = Mock::sending((0..5).map(|_| SendStep::Accept));

        let total = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .unwrap();
        assert_eq!(total, 5);
        assert_eq!(remote.sent.len(), 5);
        assert!(remote.sent.iter().all(|(buf, _)| buf == b"payload"));
    }

    #[tokio::test]
    async fn a_blocked_send_side_keeps_the_received_packets_buffered() {
        let mut client = Mock::recving([
            RecvStep::Packet(b"one", 4, 1001),
            RecvStep::Packet(b"two", 4, 1002),
            RecvStep::Eof,
        ]);
        let mut remote = Mock::sending([SendStep::Pending, SendStep::Accept, SendStep::Accept]);
        let mut relay = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(4));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut relay).poll(&mut cx).is_pending());
        let total = match Pin::new(&mut relay).poll(&mut cx) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("the relay should finish once the send side is ready"),
        };

        assert_eq!(total, 2);
        assert_eq!(
            remote.sent,
            vec![(b"one".to_vec(), ups(1001)), (b"two".to_vec(), ups(1002))]
        );
    }

    #[tokio::test]
    async fn a_client_recv_error_is_reported_as_a_client_error() {
        let mut client = Mock::recving([RecvStep::Error]);
        let mut remote = Mock::sending([]);

        let e = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .unwrap_err();
        assert!(matches!(e, UdpRelayError::ClientError(_)));
        assert!(e.to_string().starts_with("client: recv failed"));
    }

    #[tokio::test]
    async fn a_remote_send_error_is_reported_as_a_remote_error() {
        let mut client = Mock::recving([RecvStep::Packet(b"one", 4, 1001), RecvStep::Eof]);
        let mut remote = Mock::sending([SendStep::Error]);

        let e = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(2))
            .await
            .unwrap_err();
        assert!(matches!(e, UdpRelayError::RemoteError(_, _)));
        assert_eq!(e.to_string(), "remote: address not supported");
    }

    #[tokio::test]
    async fn a_relay_without_traffic_stays_idle() {
        let mut client = Mock::recving([RecvStep::Pending]);
        let mut remote = Mock::sending([]);
        let mut relay = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(2));
        assert!(relay.is_idle());

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut relay).poll(&mut cx).is_pending());
        assert!(relay.is_idle());
    }

    #[tokio::test]
    async fn a_relay_with_traffic_becomes_active_until_it_is_reset() {
        let mut client = Mock::recving([RecvStep::Packet(b"one", 4, 1001), RecvStep::Pending]);
        let mut remote = Mock::sending([SendStep::Accept]);
        let mut relay = UdpRelayClientToRemote::new(&mut client, &mut remote, test_config(2));

        let mut cx = Context::from_waker(Waker::noop());
        assert!(Pin::new(&mut relay).poll(&mut cx).is_pending());
        assert!(!relay.is_idle());

        relay.reset_active();
        assert!(relay.is_idle());
    }

    #[tokio::test]
    async fn the_relay_yields_once_the_yield_count_is_reached() {
        let mut config = test_config(8);
        config.set_yield_count(MINIMUM_UDP_RELAY_YIELD_COUNT);
        let packet_count = MINIMUM_UDP_RELAY_YIELD_COUNT + 8;

        let mut recv_steps: Vec<RecvStep> = (0..packet_count)
            .map(|_| RecvStep::Packet(b"x", 4, 1001))
            .collect();
        recv_steps.push(RecvStep::Eof);
        let mut client = Mock::recving(recv_steps);
        let mut remote = Mock::sending((0..packet_count).map(|_| SendStep::Accept));

        let mut relay = UdpRelayClientToRemote::new(&mut client, &mut remote, config);
        let mut cx = Context::from_waker(Waker::noop());
        // the first poll relays a full yield window and then hands the runtime back
        assert!(Pin::new(&mut relay).poll(&mut cx).is_pending());
        let total = match Pin::new(&mut relay).poll(&mut cx) {
            Poll::Ready(r) => r.unwrap(),
            Poll::Pending => panic!("the relay should finish on the second poll"),
        };
        assert_eq!(total, packet_count as u64);
    }

    #[test]
    fn a_remote_send_error_on_a_single_packet_carries_the_upstream() {
        let mut packet = UdpRelayPacket::new(0, 512);
        packet.set_upstream(ups(4000));
        let mut remote = Mock::sending([SendStep::Error]);

        let mut cx = Context::from_waker(Waker::noop());
        let e = match RemoteSend(&mut remote).poll_send_packet(&mut cx, &packet) {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        match e {
            UdpRelayError::RemoteError(addr, _) => assert_eq!(addr, Some(ups(4000))),
            UdpRelayError::ClientError(_) => panic!("expected a remote error"),
        }
    }

    #[test]
    fn a_client_recv_error_on_a_single_packet_is_wrapped() {
        let mut packet = UdpRelayPacket::new(0, 512);
        let mut client = Mock::recving([RecvStep::Error]);

        let mut cx = Context::from_waker(Waker::noop());
        let e = match ClientRecv(&mut client).poll_recv_packet(&mut cx, &mut packet) {
            Poll::Ready(r) => r.unwrap_err(),
            Poll::Pending => panic!("unexpected pending"),
        };
        assert!(matches!(e, UdpRelayError::ClientError(_)));
    }
}
