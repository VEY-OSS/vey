/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use log::trace;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

const UDP_BATCH_SEND_SIZE: usize = 8;

pub(super) struct Sinker {
    receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    socket: UdpSocket,
}

impl Sinker {
    pub(super) fn new(receiver: mpsc::UnboundedReceiver<Vec<u8>>, socket: UdpSocket) -> Self {
        Sinker { receiver, socket }
    }

    pub(super) async fn into_running(mut self) {
        let mut buf = Vec::with_capacity(UDP_BATCH_SEND_SIZE);
        loop {
            let nr = self.receiver.recv_many(&mut buf, UDP_BATCH_SEND_SIZE).await;
            if nr == 0 {
                break;
            }

            if let Err(e) = self.send_udp(&buf).await {
                trace!("stream dump udp send error: {e}");
            }
            buf.clear();
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
    async fn send_udp(&self, packets: &[Vec<u8>]) -> io::Result<()> {
        use std::future::poll_fn;
        use std::io::IoSlice;
        use vey_io_ext::UdpSocketExt;
        use vey_io_sys::udp::SendMsgHdr;

        let mut msgs: Vec<_> = packets
            .iter()
            .map(|v| SendMsgHdr::new([IoSlice::new(v.as_slice())], None))
            .collect();
        let mut offset = 0;
        let mut failed = None;
        while offset < msgs.len() {
            // `sendmmsg` stops at the first datagram it cannot send. Skip that one
            // so a packet over the UDP limit does not drop the rest of the batch.
            match poll_fn(|cx| self.socket.poll_batch_sendmsg(cx, &mut msgs[offset..])).await {
                Ok(0) => {
                    return Err(failed.unwrap_or_else(|| {
                        io::Error::new(io::ErrorKind::WriteZero, "sendmmsg sent no datagrams")
                    }));
                }
                Ok(n) => offset += n,
                Err(e) => {
                    failed.get_or_insert(e);
                    offset += 1;
                }
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    #[cfg(target_os = "macos")]
    async fn send_udp(&self, packets: &[Vec<u8>]) -> io::Result<()> {
        use std::future::poll_fn;
        use std::io::IoSlice;
        use vey_io_ext::UdpSocketExt;
        use vey_io_sys::udp::SendMsgHdr;

        let mut msgs: Vec<_> = packets
            .iter()
            .map(|v| SendMsgHdr::new([IoSlice::new(v.as_slice())], None))
            .collect();
        let mut offset = 0;
        let mut failed = None;
        while offset < msgs.len() {
            match poll_fn(|cx| self.socket.poll_batch_sendmsg_x(cx, &mut msgs[offset..])).await {
                Ok(0) => {
                    return Err(failed.unwrap_or_else(|| {
                        io::Error::new(io::ErrorKind::WriteZero, "sendmsg_x sent no datagrams")
                    }));
                }
                Ok(n) => offset += n,
                Err(e) => {
                    failed.get_or_insert(e);
                    offset += 1;
                }
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    #[cfg(any(windows, target_os = "dragonfly", target_os = "illumos"))]
    async fn send_udp(&self, packets: &[Vec<u8>]) -> io::Result<()> {
        let mut failed = None;
        for pkt in packets {
            if let Err(e) = self.socket.send(pkt.as_slice()).await {
                failed.get_or_insert(e);
            }
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::net::UdpSocket;
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn oversized_datagram_does_not_drop_the_rest_of_the_batch() {
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(400)))
            .unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        client.connect(server.local_addr().unwrap()).await.unwrap();

        let (_sender, receiver) = mpsc::unbounded_channel();
        let sink = Sinker::new(receiver, client);
        let packets = vec![b"one".to_vec(), vec![b'x'; 70_000], b"three".to_vec()];
        let err = sink.send_udp(&packets).await.unwrap_err();
        assert!(err.raw_os_error().is_some());

        let mut got = Vec::new();
        let mut tmp = [0u8; 16];
        while let Ok((n, _)) = server.recv_from(&mut tmp) {
            got.push(tmp[..n].to_vec());
        }
        assert_eq!(got, [b"one".to_vec(), b"three".to_vec()]);
    }
}
