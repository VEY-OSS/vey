/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::{Context, Poll, ready};
use std::{fmt, io};

use tokio::io::ReadBuf;
use tokio::net::UdpSocket;

use vey_io_sys::udp::{RecvMsgHdr, SendMsgHdr};

use super::{AsyncUdpRecv, AsyncUdpSend, UdpSocketExt};

#[derive(Debug)]
pub struct SendHalf(Arc<UdpSocket>);

#[derive(Debug)]
pub struct RecvHalf(Arc<UdpSocket>);

pub fn split(socket: UdpSocket) -> (RecvHalf, SendHalf) {
    let shared = Arc::new(socket);
    let send = shared.clone();
    let recv = shared;
    (RecvHalf(recv), SendHalf(send))
}

#[derive(Debug)]
pub struct ReuniteError(pub SendHalf, pub RecvHalf);

impl fmt::Display for ReuniteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("tried to reunite halves that are not from the same socket")
    }
}

impl Error for ReuniteError {}

fn reunite(s: SendHalf, r: RecvHalf) -> Result<UdpSocket, ReuniteError> {
    if Arc::ptr_eq(&s.0, &r.0) {
        drop(r);
        // Only two instances of the `Arc` are ever created, one for the
        // receiver and one for the sender, and those `Arc`s are never exposed
        // externally. And so when we drop one here, the other one must be the
        // only remaining one.
        Ok(Arc::try_unwrap(s.0).expect("udp: try_unwrap failed in reunite"))
    } else {
        Err(ReuniteError(s, r))
    }
}

impl SendHalf {
    pub fn reunite(self, other: RecvHalf) -> Result<UdpSocket, ReuniteError> {
        reunite(self, other)
    }
}

impl AsyncUdpSend for SendHalf {
    fn poll_send_to(
        &mut self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: SocketAddr,
    ) -> Poll<io::Result<usize>> {
        self.0.poll_send_to(cx, buf, target)
    }

    fn poll_send(&mut self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        self.0.poll_send(cx, buf)
    }

    fn poll_sendmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr: &SendMsgHdr<'_, C>,
    ) -> Poll<io::Result<usize>> {
        self.0.poll_sendmsg(cx, hdr)
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
        self.0.poll_batch_sendmsg(cx, msgs)
    }

    #[cfg(target_os = "macos")]
    fn poll_batch_sendmsg_x<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        msgs: &mut [SendMsgHdr<'_, C>],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_batch_sendmsg_x(cx, msgs)
    }
}

impl RecvHalf {
    pub fn reunite(self, other: SendHalf) -> Result<UdpSocket, ReuniteError> {
        reunite(other, self)
    }

    pub async fn connect(&self, addr: SocketAddr) -> io::Result<()> {
        self.0.connect(addr).await
    }
}

impl AsyncUdpRecv for RecvHalf {
    fn poll_recv_from(
        &mut self,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<(usize, SocketAddr)>> {
        let mut buf = ReadBuf::new(buf);
        let addr = ready!(self.0.poll_recv_from(cx, &mut buf))?;
        Poll::Ready(Ok((buf.filled().len(), addr)))
    }

    fn poll_recv(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        let mut buf = ReadBuf::new(buf);
        ready!(self.0.poll_recv(cx, &mut buf))?;
        Poll::Ready(Ok(buf.filled().len()))
    }

    fn poll_recvmsg<const C: usize>(
        &mut self,
        cx: &mut Context<'_>,
        hdr: &mut RecvMsgHdr<'_, C>,
    ) -> Poll<io::Result<()>> {
        self.0.poll_recvmsg(cx, hdr)
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
        self.0.poll_batch_recvmsg(cx, hdr_v)
    }
}

#[cfg(test)]
mod tests {
    use std::future::poll_fn;
    use std::io::{IoSlice, IoSliceMut};

    use tokio::net::UdpSocket;

    use super::*;

    async fn split_pair() -> (RecvHalf, SendHalf, SocketAddr, UdpSocket, SocketAddr) {
        let local = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let local_addr = local.local_addr().unwrap();
        let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = peer.local_addr().unwrap();
        let (recv, send) = split(local);
        (recv, send, local_addr, peer, peer_addr)
    }

    #[tokio::test]
    async fn split_halves_share_the_same_socket() {
        let (mut recv, mut send, local_addr, peer, peer_addr) = split_pair().await;

        let nw = poll_fn(|cx| send.poll_send_to(cx, b"to peer", peer_addr))
            .await
            .unwrap();
        assert_eq!(nw, 7);

        let mut buf = [0u8; 16];
        let (nr, from) = peer.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..nr], b"to peer");
        // both halves are the same socket, so the peer sees the address of the split one
        assert_eq!(from, local_addr);

        peer.send_to(b"to local", local_addr).await.unwrap();
        let (nr, from) = poll_fn(|cx| recv.poll_recv_from(cx, &mut buf))
            .await
            .unwrap();
        assert_eq!(&buf[..nr], b"to local");
        assert_eq!(from, peer_addr);
    }

    #[tokio::test]
    async fn connected_halves_send_and_recv_without_an_address() {
        let (mut recv, mut send, local_addr, peer, peer_addr) = split_pair().await;
        recv.connect(peer_addr).await.unwrap();
        peer.connect(local_addr).await.unwrap();

        let nw = poll_fn(|cx| send.poll_send(cx, b"ping")).await.unwrap();
        assert_eq!(nw, 4);

        let mut buf = [0u8; 16];
        let nr = peer.recv(&mut buf).await.unwrap();
        assert_eq!(&buf[..nr], b"ping");

        peer.send(b"pong").await.unwrap();
        let nr = poll_fn(|cx| recv.poll_recv(cx, &mut buf)).await.unwrap();
        assert_eq!(&buf[..nr], b"pong");
    }

    #[tokio::test]
    async fn sendmsg_and_recvmsg_go_through_the_halves() {
        let (mut recv, mut send, local_addr, peer, peer_addr) = split_pair().await;

        let hdr = SendMsgHdr::new([IoSlice::new(b"ab"), IoSlice::new(b"cd")], Some(peer_addr));
        let nw = poll_fn(|cx| send.poll_sendmsg(cx, &hdr)).await.unwrap();
        assert_eq!(nw, 4);

        let mut buf = [0u8; 16];
        let (nr, _) = peer.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..nr], b"abcd");

        peer.send_to(b"efgh", local_addr).await.unwrap();
        let mut recv_buf = [0u8; 16];
        let mut hdr = RecvMsgHdr::new([IoSliceMut::new(&mut recv_buf)]);
        poll_fn(|cx| recv.poll_recvmsg(cx, &mut hdr)).await.unwrap();
        assert_eq!(hdr.n_recv, 4);
        assert_eq!(hdr.src_addr(), Some(peer_addr));
        assert_eq!(&recv_buf[..4], b"efgh");
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
    async fn batch_sendmsg_and_batch_recvmsg_go_through_the_halves() {
        let (mut recv, mut send, local_addr, peer, peer_addr) = split_pair().await;

        let mut msgs = [
            SendMsgHdr::new([IoSlice::new(b"one")], Some(peer_addr)),
            SendMsgHdr::new([IoSlice::new(b"two")], Some(peer_addr)),
        ];
        let count = poll_fn(|cx| send.poll_batch_sendmsg(cx, &mut msgs))
            .await
            .unwrap();
        assert_eq!(count, 2);

        let mut buf = [0u8; 16];
        for expect in [b"one", b"two"] {
            let (nr, _) = peer.recv_from(&mut buf).await.unwrap();
            assert_eq!(&buf[..nr], expect);
        }

        peer.send_to(b"back", local_addr).await.unwrap();
        let mut recv_buf = [0u8; 16];
        let mut hdr_v = [RecvMsgHdr::new([IoSliceMut::new(&mut recv_buf)])];
        let count = poll_fn(|cx| recv.poll_batch_recvmsg(cx, &mut hdr_v))
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(hdr_v[0].n_recv, 4);
        assert_eq!(&recv_buf[..4], b"back");
    }

    /// macOS `sendmsg_x` only works on connected sockets, so the destination is taken from
    /// the connection rather than from each `SendMsgHdr`.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn batch_sendmsg_x_and_batch_recvmsg_go_through_the_halves() {
        let (mut recv, mut send, local_addr, peer, peer_addr) = split_pair().await;
        recv.connect(peer_addr).await.unwrap();

        let mut msgs = [
            SendMsgHdr::new([IoSlice::new(b"one")], None),
            SendMsgHdr::new([IoSlice::new(b"two")], None),
        ];
        let count = poll_fn(|cx| send.poll_batch_sendmsg_x(cx, &mut msgs))
            .await
            .unwrap();
        assert_eq!(count, 2);

        let mut buf = [0u8; 16];
        for expect in [b"one", b"two"] {
            let (nr, _) = peer.recv_from(&mut buf).await.unwrap();
            assert_eq!(&buf[..nr], expect);
        }

        peer.send_to(b"back", local_addr).await.unwrap();
        let mut recv_buf = [0u8; 16];
        let mut hdr_v = [RecvMsgHdr::new([IoSliceMut::new(&mut recv_buf)])];
        let count = poll_fn(|cx| recv.poll_batch_recvmsg(cx, &mut hdr_v))
            .await
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(hdr_v[0].n_recv, 4);
        assert_eq!(&recv_buf[..4], b"back");
    }

    #[tokio::test]
    async fn recv_half_reunites_with_its_send_half() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let (recv, send) = split(socket);
        let reunited = recv.reunite(send).unwrap();
        assert_eq!(reunited.local_addr().unwrap(), addr);
    }

    #[tokio::test]
    async fn reunite_error_keeps_both_halves() {
        let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let a_addr = a.local_addr().unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let (ra, sa) = split(a);
        let (rb, sb) = split(b);

        let ReuniteError(sa, rb) = sa.reunite(rb).unwrap_err();
        // the returned halves are still usable, each with its own socket
        assert_eq!(sa.reunite(ra).unwrap().local_addr().unwrap(), a_addr);
        assert!(rb.reunite(sb).is_ok());
    }

    #[tokio::test]
    async fn reunite_same_socket_succeeds() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = socket.local_addr().unwrap();
        let (recv, send) = split(socket);
        let reunited = send.reunite(recv).unwrap();
        assert_eq!(reunited.local_addr().unwrap(), addr);
    }

    #[tokio::test]
    async fn reunite_different_sockets_fails() {
        let a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let b = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let (_ra, sa) = split(a);
        let (rb, _sb) = split(b);
        let err = sa.reunite(rb).unwrap_err();
        assert_eq!(
            err.to_string(),
            "tried to reunite halves that are not from the same socket"
        );
    }
}
