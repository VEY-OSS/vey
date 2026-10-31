/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::{SocketAddr, UdpSocket};

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "solaris",
))]
use vey_io_sys::udp::{SendMsgHdr, UdpSocketExt};

use super::SinkBuf;

pub(super) struct UdpMetricsSink {
    addr: SocketAddr,
    socket: UdpSocket,
    max_segment_size: usize,
}

impl UdpMetricsSink {
    pub(super) fn new(
        addr: SocketAddr,
        socket: UdpSocket,
        max_segment_size: Option<usize>,
    ) -> Self {
        UdpMetricsSink {
            addr,
            socket,
            max_segment_size: max_segment_size.unwrap_or(1400),
        }
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    )))]
    pub(super) fn send_batch(&self, buf: &mut SinkBuf) -> io::Result<()> {
        for packet in buf.iter(self.max_segment_size) {
            self.socket.send_to(packet.as_ref(), self.addr)?;
        }
        Ok(())
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    pub(super) fn send_batch(&self, buf: &mut SinkBuf) -> io::Result<()> {
        const MAX_BATCH_SIZE: usize = 32;

        let mut hdrs: [SendMsgHdr<'_, 1>; MAX_BATCH_SIZE] = unsafe { std::mem::zeroed() };

        let mut offset = 0usize;
        let mut failed = None;
        for packet in buf.iter(self.max_segment_size) {
            hdrs[offset] = SendMsgHdr::new([packet], Some(self.addr));
            offset += 1;
            if offset >= MAX_BATCH_SIZE {
                if let Err(e) = self.send_headers(&mut hdrs) {
                    failed.get_or_insert(e);
                }
                offset = 0;
            }
        }
        if offset > 0
            && let Err(e) = self.send_headers(&mut hdrs[..offset])
        {
            failed.get_or_insert(e);
        }
        match failed {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// `sendmmsg` returns how many datagrams were sent. A later datagram can fail
    /// (for example it is larger than the UDP limit) after earlier ones succeeded.
    /// Skip that datagram and keep sending the rest, and surface the error.
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    ))]
    fn send_headers(&self, hdrs: &mut [SendMsgHdr<'_, 1>]) -> io::Result<()> {
        let mut offset = 0;
        let mut failed = None;
        while offset < hdrs.len() {
            match self.socket.batch_sendmsg(&mut hdrs[offset..]) {
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
}

#[cfg(all(
    test,
    any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
    )
))]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::sink::buf::SinkBuf;

    #[test]
    fn oversized_datagram_does_not_drop_the_rest_of_the_batch() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        let sink = UdpMetricsSink::new(server.local_addr().unwrap(), client, Some(100));

        let mut buf = SinkBuf::new(80_000);
        buf.receive(|b| b.extend_from_slice(b"one"));
        buf.receive(|b| b.resize(b.len() + 70_000, b'x'));
        buf.receive(|b| b.extend_from_slice(b"three"));

        let err = sink.send_batch(&mut buf).unwrap_err();
        assert!(err.raw_os_error().is_some());

        let mut got = Vec::new();
        let mut tmp = [0u8; 16];
        while let Ok((n, _)) = server.recv_from(&mut tmp) {
            got.push(tmp[..n].to_vec());
        }
        assert_eq!(got, [b"one".to_vec(), b"three".to_vec()]);
    }
}
