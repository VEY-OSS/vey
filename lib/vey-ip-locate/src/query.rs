/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::io;
use std::net::IpAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::anyhow;
use log::warn;
use tokio::io::ReadBuf;
use tokio::net::UdpSocket;

use super::{
    IpLocateServiceConfig, IpLocationCacheResponse, IpLocationQueryHandle, Request, Response,
};

pub(crate) struct IpLocationQueryRuntime {
    socket: UdpSocket,
    query_handle: IpLocationQueryHandle,
    read_buffer: Box<[u8]>,
    write_queue: VecDeque<(IpAddr, Vec<u8>)>,
    default_expire_ttl: u32,
    maximum_expire_ttl: u32,
    query_wait: Duration,
}

impl IpLocationQueryRuntime {
    pub(crate) fn new(
        config: &IpLocateServiceConfig,
        socket: UdpSocket,
        query_handle: IpLocationQueryHandle,
    ) -> Self {
        IpLocationQueryRuntime {
            socket,
            query_handle,
            // SAFETY: only `read_buffer[..len]` is parsed after poll_recv fills it.
            read_buffer: unsafe { Box::<[u8]>::new_uninit_slice(16384).assume_init() },
            write_queue: VecDeque::new(),
            default_expire_ttl: config.default_expire_ttl,
            maximum_expire_ttl: config.maximum_expire_ttl,
            query_wait: config.query_wait_timeout,
        }
    }

    fn send_empty_result(&mut self, ip: IpAddr, ttl: u32, expired: bool) {
        let result = IpLocationCacheResponse::empty(ttl);
        self.query_handle.send_rsp_data(Some(ip), result, expired);
    }

    fn send_expire_ttl(&mut self, ttl: u32) {
        let result = IpLocationCacheResponse::empty(ttl);
        self.query_handle.send_rsp_data(None, result, false);
    }

    fn handle_req(&mut self, ip: IpAddr) {
        if self.query_handle.should_send_raw_query(ip, self.query_wait) {
            match Request::encode_new(ip) {
                Ok(buf) => self.write_queue.push_back((ip, buf)),
                Err(_) => self.send_empty_result(ip, self.default_expire_ttl, false),
            }
        }
    }

    fn handle_rsp(&mut self, len: usize) {
        let mut buf = &self.read_buffer[..len];
        match rmpv::decode::read_value_ref(&mut buf)
            .map_err(|e| anyhow!("invalid msgpack response data: {e}"))
            .and_then(|v| Response::parse(v))
            .map(|r| r.into_parts())
        {
            Ok((ip, location, ttl)) => {
                let ttl = ttl
                    .unwrap_or(self.default_expire_ttl)
                    .min(self.maximum_expire_ttl);

                if let Some(location) = location {
                    let result = IpLocationCacheResponse::new(location, ttl);
                    self.query_handle.send_rsp_data(ip, result, false);
                } else if let Some(ip) = ip {
                    self.send_empty_result(ip, ttl, false);
                } else {
                    self.send_expire_ttl(ttl);
                }
            }
            Err(e) => {
                warn!("parse cert generator rsp error: {e:?}");
            }
        }
    }

    fn poll_loop(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // A connected UDP socket reports ICMP failures as recv errors. One
        // such error must not end this task: dropping it closes the response
        // channel and the cache runtime stops with it.
        let mut recv_error_streak = 0u8;
        loop {
            // handle rsp
            let mut buf = ReadBuf::new(&mut self.read_buffer);
            match self.socket.poll_recv(cx, &mut buf) {
                Poll::Pending => {}
                Poll::Ready(Err(e)) => {
                    warn!("socket recv error: {e:?}");
                    recv_error_streak = recv_error_streak.saturating_add(1);
                    if recv_error_streak > 1 {
                        // The error is still pending. Yield instead of spinning.
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    continue;
                }
                Poll::Ready(Ok(_)) => {
                    recv_error_streak = 0;
                    let len = buf.filled().len();
                    if len > 0 {
                        self.handle_rsp(len);
                    }
                    continue;
                }
            }

            // send req from write queue
            while let Some((ip, buf)) = self.write_queue.pop_front() {
                match self.socket.poll_send(cx, &buf) {
                    Poll::Pending => {
                        self.write_queue.push_front((ip, buf));
                        break;
                    }
                    Poll::Ready(Ok(_)) => {}
                    Poll::Ready(Err(_)) => {
                        self.send_empty_result(ip, self.default_expire_ttl, false)
                    }
                }
            }

            // handle timeout
            loop {
                match self.query_handle.poll_query_expired(cx) {
                    Poll::Pending => break,
                    Poll::Ready(None) => break,
                    Poll::Ready(Some(ip)) => {
                        self.send_empty_result(ip, self.default_expire_ttl, true)
                    }
                }
            }

            // handle req
            match self.query_handle.poll_recv_req(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Ready(Some(req)) => self.handle_req(req),
            }
        }
    }
}

impl Future for IpLocationQueryRuntime {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        (*self).poll_loop(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    use tokio::sync::mpsc;

    #[tokio::test]
    async fn recv_error_does_not_stop_the_query_runtime() {
        let mut config = IpLocateServiceConfig::default();
        config.set_query_wait_timeout(Duration::from_secs(30));

        let closed = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let closed_addr = closed.local_addr().unwrap();
        drop(closed);

        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        socket.connect(closed_addr).await.unwrap();

        let (query_sender, query_receiver) = mpsc::unbounded_channel();
        let (rsp_sender, mut rsp_receiver) = mpsc::unbounded_channel();
        let query_handle = IpLocationQueryHandle::new(query_receiver, rsp_sender);
        let task = tokio::spawn(IpLocationQueryRuntime::new(&config, socket, query_handle));

        query_sender
            .send(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)))
            .unwrap();

        // Sending to the closed port produces an ICMP error on the next recv.
        let closed = tokio::time::timeout(Duration::from_secs(1), rsp_receiver.recv()).await;
        assert!(closed.is_err(), "recv error closed the response channel");
        assert!(!task.is_finished());

        drop(query_sender);
        let finished = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("query runtime did not stop after the request channel closed")
            .unwrap();
        assert!(finished.is_ok());
    }
}
