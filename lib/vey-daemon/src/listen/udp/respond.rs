/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! One UDP datagram in, one datagram back.
//!
//! This runtime is for a protocol whose request and response each fit in a single datagram.

use std::future::{Future, poll_fn};
use std::io::{self, IoSlice, IoSliceMut};
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::{Context, Poll};

#[cfg(feature = "ebpf")]
use anyhow::anyhow;
use log::{info, warn};
use smallvec::SmallVec;
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};

use vey_io_ext::UdpSocketExt;
use vey_io_sys::udp::{RecvMsgHdr, SendMsgHdr};
#[cfg(feature = "ebpf")]
use vey_reuseport::udp::{UdpSocketSelectGuard, UdpSocketSelector};
#[cfg(feature = "ebpf")]
use vey_socket::RawSocket;
#[cfg(feature = "ebpf")]
use vey_types::net::UdpConnectionTrackConfig;
use vey_types::net::UdpListenConfig;

use crate::listen::{ListenAliveGuard, ListenStats};
use crate::server::{BaseServer, ReloadServer, ServerReloadCommand};

const REPLY_QUEUE: usize = 512;
const REPLY_BATCH: usize = 16;
const RECV_BATCH: usize = 16;

struct PendingReply {
    client_addr: SocketAddr,
    packet: Vec<u8>,
}

/// One response to one client.
///
/// The socket task batches every reply queued on this listen socket. This
/// value only carries the destination for its own packet.
#[derive(Clone)]
pub struct UdpResponder {
    client_addr: SocketAddr,
    server_addr: SocketAddr,
    tx: mpsc::Sender<PendingReply>,
}

impl UdpResponder {
    pub fn client_addr(&self) -> SocketAddr {
        self.client_addr
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.server_addr
    }

    /// Queue this one packet. The socket task writes it later.
    pub async fn send(&self, packet: &[u8]) -> io::Result<()> {
        self.tx
            .send(PendingReply {
                client_addr: self.client_addr,
                packet: packet.to_vec(),
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "responder closed"))
    }
}

/// Handles one request datagram.
///
/// The runtime does not write the response itself. [`UdpResponder::send`]
/// queues the one packet for `responder`'s client. The socket task batches
/// those queued packets.
pub trait RespondUdpServer: BaseServer {
    fn handle_udp_request<'a>(
        &'a self,
        packet: &'a [u8],
        responder: UdpResponder,
        worker_id: Option<usize>,
    ) -> impl Future<Output = ()> + Send + 'a;
}

/// Batches replies queued on one listen socket and writes them with `sendmmsg`.
struct PacketRecvSender {
    socket: Arc<UdpSocket>,
    reply_receiver: mpsc::Receiver<PendingReply>,
    listen_stats: Arc<ListenStats>,
}

impl PacketRecvSender {
    fn reply_hdr(reply: &PendingReply) -> SendMsgHdr<'_, 1> {
        SendMsgHdr::new(
            [IoSlice::new(reply.packet.as_slice())],
            Some(reply.client_addr),
        )
    }

    async fn run_to_end(mut self) {
        let mut queued = Vec::with_capacity(REPLY_BATCH);
        loop {
            queued.clear();
            let n = self
                .reply_receiver
                .recv_many(&mut queued, REPLY_BATCH)
                .await;
            if n == 0 {
                break;
            }
            self.listen_stats.add_datagram_send_queued(n as u64);
            self.send_packets(&queued).await;
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
    async fn send_packets(&self, queued: &[PendingReply]) {
        let mut logged = false;
        for reply in queued {
            if let Err(e) = poll_fn(|cx| {
                let hdr = Self::reply_hdr(reply);
                self.socket.poll_sendmsg(cx, &hdr)
            })
            .await
            {
                self.listen_stats.add_datagram_send_failed(1);
                if !logged {
                    warn!("udp respond send failed: {e}");
                    logged = true;
                }
            } else {
                self.listen_stats.add_datagram_send_ok(1);
            }
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
    async fn send_packets(&self, queued: &[PendingReply]) {
        let mut offset = 0;
        let mut logged = false;
        while offset < queued.len() {
            match poll_fn(|cx| {
                let mut headers: SmallVec<[SendMsgHdr<1>; REPLY_BATCH]> = SmallVec::new();
                for reply in &queued[offset..] {
                    headers.push(Self::reply_hdr(reply));
                }
                self.socket.poll_batch_sendmsg(cx, &mut headers)
            })
            .await
            {
                Ok(n) => {
                    self.listen_stats.add_datagram_send_ok(n as u64);
                    offset += n;
                }
                Err(e) => {
                    self.listen_stats.add_datagram_send_failed(1);
                    if !logged {
                        warn!("udp respond send failed: {e}");
                        logged = true;
                    }
                    offset += 1;
                }
            }
        }
    }
}

/// Binds one UDP socket per instance and runs a responder task on each.
///
/// On Linux with the `ebpf` feature, `run_all_instances` attaches the reuseport
/// selector used for a lossless upgrade. The new process steers new clients to
/// itself. Clients already stuck to a live socket stay there until that socket
/// leaves the map, and the kernel then hands the same packet to a live socket.
pub struct RespondUdpRuntime<S> {
    server: S,
    listen_config: UdpListenConfig,
    listen_stats: Arc<ListenStats>,
    packet_max_size: u16,
    #[cfg(feature = "ebpf")]
    socket_selector: Option<UdpSocketSelector>,
}

impl<S> RespondUdpRuntime<S>
where
    S: RespondUdpServer + ReloadServer + Clone + Send + Sync + 'static,
{
    /// `packet_max_size` is the buffer for one datagram. A larger datagram is truncated.
    pub fn new(
        server: S,
        listen_config: UdpListenConfig,
        listen_stats: Arc<ListenStats>,
        packet_max_size: u16,
    ) -> Self {
        RespondUdpRuntime {
            server,
            listen_config,
            listen_stats,
            packet_max_size,
            #[cfg(feature = "ebpf")]
            socket_selector: None,
        }
    }

    fn create_instance(
        &self,
        id: usize,
        listen_addr: SocketAddr,
        listen_in_worker: bool,
    ) -> RespondUdpRuntimeInstance<S> {
        let server_type = self.server.r#type();
        let server_version = self.server.version();

        let n = self.packet_max_size as usize;
        let mut packets_buf = Vec::with_capacity(RECV_BATCH);
        for _ in 0..RECV_BATCH {
            packets_buf.push(vec![0u8; n]);
        }

        RespondUdpRuntimeInstance {
            server: self.server.clone(),
            server_type,
            server_version,
            worker_id: None,
            listen_addr,
            listen_in_worker,
            instance_id: id,
            packets_buf,
            listen_stats: Arc::clone(&self.listen_stats),
            _alive_guard: None,
            #[cfg(feature = "ebpf")]
            _bpf_guard: None,
        }
    }

    /// Bind one socket per instance and run a responder task on each.
    ///
    /// `listen_in_worker` places one instance on every worker when workers
    /// exist. Each task has its own socket; reply on the [`UdpResponder`]
    /// delivered to that task so the response leaves the socket that received
    /// the request.
    pub fn run_all_instances(
        &mut self,
        listen_in_worker: bool,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand<()>>,
    ) -> anyhow::Result<()> {
        let mut instance_count = self.listen_config.instance();
        if listen_in_worker {
            let worker_count = crate::runtime::worker::worker_count();
            if worker_count > 0 {
                instance_count = worker_count;
            }
        }

        #[cfg(feature = "ebpf")]
        if self
            .listen_config
            .use_ebpf(rustix::process::getuid().as_raw())
        {
            match UdpSocketSelector::new(
                rustix::process::getpid().as_raw_pid(),
                self.server.version() as u16,
                self.listen_config.address(),
                UdpConnectionTrackConfig::default().ebpf_conn_track_size(),
            ) {
                Ok(selector) => {
                    self.socket_selector = Some(selector);
                }
                Err(e) => {
                    if self.listen_config.fail_on_ebpf_error() {
                        return Err(anyhow!(
                            "UDP {} ebpf reuseport socket selector create failed: {e}",
                            self.listen_config.address()
                        ));
                    }
                    warn!(
                        "reuseport ebpf on UDP socket {} disabled due to create error {e}",
                        self.listen_config.address()
                    );
                }
            }
        }

        for i in 0..instance_count {
            let socket = vey_socket::udp::new_std_bind_listen(&self.listen_config)?;
            let listen_addr = socket.local_addr()?;

            #[cfg(feature = "ebpf")]
            let mut instance = self.create_instance(i, listen_addr, listen_in_worker);
            #[cfg(not(feature = "ebpf"))]
            let instance = self.create_instance(i, listen_addr, listen_in_worker);
            #[cfg(feature = "ebpf")]
            if let Some(selector) = &mut self.socket_selector {
                let guard = selector.add_socket(RawSocket::from(&socket));
                instance._bpf_guard = Some(guard);
            }
            instance.into_running(socket, server_reload_sender.subscribe());
        }

        #[cfg(feature = "ebpf")]
        if let Some(mut selector) = self.socket_selector.take() {
            if let Err(e) = selector.load_and_attach() {
                if self.listen_config.fail_on_ebpf_error() {
                    return Err(anyhow!(
                        "UDP {} ebpf reuseport socket selector attach failed: {e}",
                        self.listen_config.address()
                    ));
                }
                warn!(
                    "reuseport ebpf on UDP socket {} disabled due to attach error {e}",
                    self.listen_config.address()
                );
            }
            let mut server_reload_receiver = server_reload_sender.subscribe();
            tokio::spawn(async move {
                while let Ok(cmd) = server_reload_receiver.recv().await {
                    if matches!(cmd, ServerReloadCommand::QuitRuntime) {
                        break;
                    }
                }
                selector.unregister_proc();
            });
        }

        Ok(())
    }
}

/// Receives a batch of request datagrams and hands each one to a [`UdpResponder`].
struct RespondUdpRuntimeInstance<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    worker_id: Option<usize>,
    listen_addr: SocketAddr,
    listen_in_worker: bool,
    instance_id: usize,
    packets_buf: Vec<Vec<u8>>,
    listen_stats: Arc<ListenStats>,
    _alive_guard: Option<ListenAliveGuard>,
    #[cfg(feature = "ebpf")]
    _bpf_guard: Option<UdpSocketSelectGuard>,
}

struct InboundDatagram {
    index: usize,
    len: usize,
    client_addr: SocketAddr,
    server_addr: SocketAddr,
}

impl<S> RespondUdpRuntimeInstance<S>
where
    S: RespondUdpServer + ReloadServer + Clone + Send + Sync + 'static,
{
    fn pre_start(&mut self) {
        info!(
            "started {} SRT[{}_v{}#{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
            self.instance_id,
        );
        self._alive_guard = Some(self.listen_stats.add_running_runtime());
    }

    fn pre_stop(&self) {
        info!(
            "stopping {} SRT[{}_v{}#{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
            self.instance_id,
        );
    }

    fn post_stop(&self) {
        info!(
            "stopped {} SRT[{}_v{}#{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
            self.instance_id,
        );
    }

    async fn run(
        mut self,
        socket: UdpSocket,
        mut server_reload_channel: broadcast::Receiver<ServerReloadCommand<()>>,
    ) {
        use broadcast::error::RecvError;

        let socket = Arc::new(socket);
        let (reply_tx, reply_rx) = mpsc::channel(REPLY_QUEUE);
        let packet_recv_sender = PacketRecvSender {
            socket: Arc::clone(&socket),
            reply_receiver: reply_rx,
            listen_stats: Arc::clone(&self.listen_stats),
        };
        let reply_task = tokio::spawn(packet_recv_sender.run_to_end());
        loop {
            // Recv and quit are polled together. A busy socket must not hide shutdown.
            tokio::select! {
                ev = server_reload_channel.recv() => {
                    match ev {
                        Ok(ServerReloadCommand::ReloadVersion(version)) => {
                            info!("SRT[{}_v{}#{}] received reload request from v{version}",
                                self.server.name(), self.server_version, self.instance_id);
                            let new_server = self.server.reload();
                            self.server_version = new_server.version();
                            self.server = new_server;
                            continue;
                        }
                        Ok(ServerReloadCommand::UpdateInPlace(_c)) => {
                            continue;
                        }
                        Ok(ServerReloadCommand::QuitRuntime) => {},
                        Err(RecvError::Closed) => {},
                        Err(RecvError::Lagged(dropped)) => {
                            warn!("SRT[{}_v{}#{}] server {} reload notify channel overflowed, {dropped} msg dropped",
                                self.server.name(), self.server_version, self.instance_id, self.server.name());
                            continue;
                        }
                    }

                    info!("SRT[{}_v{}#{}] will go offline",
                        self.server.name(), self.server_version, self.instance_id);
                    self.pre_stop();
                    break;
                }
                r = self.recv_packets(&socket) => {
                    match r {
                        Ok(packets) => self.handle_inbound(packets, &reply_tx).await,
                        Err(e) => {
                            warn!("SRT[{}_v{}#{}] error receiving data from socket, error: {e}",
                                self.server.name(), self.server_version, self.instance_id);
                        }
                    }
                }
            }
        }

        #[cfg(feature = "ebpf")]
        self.drain_after_reuseport_detach(&socket, &reply_tx).await;

        drop(reply_tx);
        if let Err(e) = reply_task.await {
            warn!(
                "SRT[{}_v{}#{}] reply task ended with error: {e}",
                self.server.name(),
                self.server_version,
                self.instance_id
            );
        }

        self.post_stop();
    }

    /// Drop this process out of the reuseport map, then read datagrams already
    /// queued on the socket so closing the fd does not discard them.
    #[cfg(feature = "ebpf")]
    async fn drain_after_reuseport_detach(
        &mut self,
        socket: &UdpSocket,
        reply_tx: &mpsc::Sender<PendingReply>,
    ) {
        if self._bpf_guard.is_none() {
            return;
        }
        drop(self._bpf_guard.take());

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(200);
        loop {
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(left) => break,
                r = self.recv_packets(socket) => {
                    match r {
                        Ok(packets) => self.handle_inbound(packets, reply_tx).await,
                        Err(_) => break,
                    }
                }
            }
        }
    }

    async fn recv_packets(
        &mut self,
        socket: &UdpSocket,
    ) -> io::Result<SmallVec<[InboundDatagram; RECV_BATCH]>> {
        poll_fn(|cx| self.poll_recv_packets(cx, socket)).await
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
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context<'_>,
        socket: &UdpSocket,
    ) -> Poll<io::Result<SmallVec<[InboundDatagram; RECV_BATCH]>>> {
        let mut nr = 0usize;
        let mut packets = SmallVec::new();
        while nr < RECV_BATCH {
            let index = packets.len();
            let mut hdr = RecvMsgHdr::new([IoSliceMut::new(&mut self.packets_buf[index])]);
            match socket.poll_recvmsg(cx, &mut hdr) {
                Poll::Ready(Ok(())) => {
                    nr += 1;
                    self.listen_stats.add_datagram_recv_total(1);
                    let Some(client_addr) = hdr.src_addr() else {
                        self.listen_stats.add_datagram_recv_dropped(1);
                        continue;
                    };
                    let server_addr = hdr.dst_addr(self.listen_addr);
                    packets.push(InboundDatagram {
                        index,
                        len: hdr.n_recv,
                        client_addr,
                        server_addr,
                    });
                }
                Poll::Ready(Err(e)) => {
                    if nr == 0 {
                        return Poll::Ready(Err(e));
                    }
                    break;
                }
                Poll::Pending => {
                    if nr == 0 {
                        return Poll::Pending;
                    }
                    break;
                }
            }
        }
        Poll::Ready(Ok(packets))
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
        socket: &UdpSocket,
    ) -> Poll<io::Result<SmallVec<[InboundDatagram; RECV_BATCH]>>> {
        let listen_stats = &self.listen_stats;
        let mut hdr_v = SmallVec::<[RecvMsgHdr<1>; RECV_BATCH]>::new();
        for buf in &mut self.packets_buf {
            hdr_v.push(RecvMsgHdr::new([IoSliceMut::new(buf)]));
        }

        let nr = std::task::ready!(socket.poll_batch_recvmsg(cx, &mut hdr_v))?;
        listen_stats.add_datagram_recv_total(nr as u64);
        let mut packets = SmallVec::new();
        for (index, hdr) in hdr_v.iter().take(nr).enumerate() {
            let Some(client_addr) = hdr.src_addr() else {
                listen_stats.add_datagram_recv_dropped(1);
                continue;
            };
            let server_addr = hdr.dst_addr(self.listen_addr);
            packets.push(InboundDatagram {
                index,
                len: hdr.n_recv,
                client_addr,
                server_addr,
            });
        }
        Poll::Ready(Ok(packets))
    }

    async fn handle_inbound(
        &mut self,
        packets: SmallVec<[InboundDatagram; RECV_BATCH]>,
        reply_tx: &mpsc::Sender<PendingReply>,
    ) {
        for pkt in packets {
            let responder = UdpResponder {
                client_addr: pkt.client_addr,
                server_addr: pkt.server_addr,
                tx: reply_tx.clone(),
            };
            let worker_id = self.worker_id;
            let server = &self.server;
            let packet = &self.packets_buf[pkt.index][..pkt.len];
            self.listen_stats.add_datagram_recv_ok(1);
            server
                .handle_udp_request(packet, responder, worker_id)
                .await;
        }
    }

    fn get_rt_handle(&mut self) -> Handle {
        if self.listen_in_worker
            && let Some(rt) = crate::runtime::worker::select_listen_handle()
        {
            self.worker_id = Some(rt.id);
            return rt.handle;
        }
        Handle::current()
    }

    fn into_running(
        mut self,
        socket: std::net::UdpSocket,
        server_reload_channel: broadcast::Receiver<ServerReloadCommand<()>>,
    ) {
        let handle = self.get_rt_handle();
        handle.spawn(async move {
            // make sure the listen socket associated with the correct reactor
            match UdpSocket::from_std(socket) {
                Ok(socket) => {
                    self.pre_start();
                    self.run(socket, server_reload_channel).await;
                }
                Err(e) => {
                    warn!(
                        "SRT[{}_v{}#{}] udp bind async: {e:?}",
                        self.server.name(),
                        self.server_version,
                        self.instance_id
                    );
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::str::FromStr;
    use std::sync::Mutex;
    use std::time::Duration;

    use tokio::net::UdpSocket as TokioUdpSocket;
    use tokio::time::timeout;
    use vey_types::metrics::NodeName;

    type SeenDatagram = (Vec<u8>, SocketAddr, SocketAddr);
    type SeenDatagrams = Arc<Mutex<Vec<SeenDatagram>>>;

    #[derive(Clone)]
    struct TestServer {
        name: NodeName,
        seen: SeenDatagrams,
    }

    impl BaseServer for TestServer {
        fn name(&self) -> &NodeName {
            &self.name
        }

        fn r#type(&self) -> &'static str {
            "test-respond-udp"
        }

        fn version(&self) -> usize {
            1
        }
    }

    impl ReloadServer for TestServer {
        fn reload(&self) -> Self {
            self.clone()
        }
    }

    impl RespondUdpServer for TestServer {
        fn handle_udp_request<'a>(
            &'a self,
            packet: &'a [u8],
            responder: UdpResponder,
            worker_id: Option<usize>,
        ) -> impl Future<Output = ()> + Send + 'a {
            self.seen.lock().unwrap().push((
                packet.to_vec(),
                responder.client_addr(),
                responder.server_addr(),
            ));
            async move {
                let _ = worker_id;
                responder.send(b"pong").await.unwrap();
            }
        }
    }

    async fn recv_from(socket: &TokioUdpSocket) -> (Vec<u8>, SocketAddr) {
        let mut buf = [0u8; 16];
        let (n, from) = timeout(Duration::from_secs(2), socket.recv_from(&mut buf))
            .await
            .expect("timed out waiting for datagram")
            .unwrap();
        (buf[..n].to_vec(), from)
    }

    #[tokio::test]
    async fn response_goes_to_the_named_client() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let server = TestServer {
            name: NodeName::from_str("respond-udp").unwrap(),
            seen: seen.clone(),
        };
        let listen_std = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        listen_std.set_nonblocking(true).unwrap();
        let listen_addr = listen_std.local_addr().unwrap();

        let listen_stats = Arc::new(ListenStats::new(Default::default()));
        let runtime = RespondUdpRuntime::new(
            server,
            UdpListenConfig::new(listen_addr),
            Arc::clone(&listen_stats),
            4096,
        );
        let instance = runtime.create_instance(0, listen_addr, false);
        let (reload, reload_rx) = broadcast::channel(4);
        instance.into_running(listen_std, reload_rx);

        let sender = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let other = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();

        sender.send_to(b"ping", listen_addr).await.unwrap();

        let (body, from) = recv_from(&sender).await;
        assert_eq!(body, b"pong");
        assert_eq!(from, listen_addr);

        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].0, b"ping");
            assert_eq!(seen[0].1, sender.local_addr().unwrap());
            assert_eq!(seen[0].2, listen_addr);
        }

        let mut buf = [0u8; 16];
        let missed = timeout(Duration::from_millis(200), other.recv_from(&mut buf)).await;
        assert!(missed.is_err(), "other should not receive the reply");
        assert!(listen_stats.is_running());
        assert_eq!(listen_stats.datagram_recv_total(), 1);
        assert_eq!(listen_stats.datagram_recv_dropped(), 0);
        assert_eq!(listen_stats.datagram_recv_ok(), 1);
        assert_eq!(listen_stats.datagram_send_queued(), 1);
        assert_eq!(listen_stats.datagram_send_failed(), 0);
        assert_eq!(listen_stats.datagram_send_ok(), 1);

        let _ = reload.send(ServerReloadCommand::QuitRuntime);
        timeout(Duration::from_secs(2), async {
            while listen_stats.is_running() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("runtime did not stop");
    }

    #[tokio::test]
    async fn recv_packets_returns_each_queued_datagram() {
        let listen_std = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        listen_std.set_nonblocking(true).unwrap();
        let listen_addr = listen_std.local_addr().unwrap();

        let sender = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let sender_addr = sender.local_addr().unwrap();
        let payloads: [&[u8]; 4] = [b"a", b"bb", b"ccc", b"dddd"];
        for payload in payloads {
            sender.send_to(payload, listen_addr).await.unwrap();
        }

        let socket = UdpSocket::from_std(listen_std).unwrap();
        let server = TestServer {
            name: NodeName::from_str("respond-udp").unwrap(),
            seen: Arc::new(Mutex::new(Vec::new())),
        };
        let runtime = RespondUdpRuntime::new(
            server,
            UdpListenConfig::new(listen_addr),
            Arc::new(ListenStats::new(Default::default())),
            4096,
        );
        let mut instance = runtime.create_instance(0, listen_addr, false);

        let packets = timeout(Duration::from_secs(2), instance.recv_packets(&socket))
            .await
            .expect("timed out waiting for datagrams")
            .unwrap();
        assert_eq!(packets.len(), payloads.len());
        for (pkt, payload) in packets.iter().zip(payloads) {
            assert_eq!(pkt.client_addr, sender_addr);
            assert_eq!(pkt.server_addr, listen_addr);
            assert_eq!(&instance.packets_buf[pkt.index][..pkt.len], payload);
        }
    }

    #[tokio::test]
    async fn each_queued_datagram_gets_a_reply() {
        let listen_std = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        listen_std.set_nonblocking(true).unwrap();
        let listen_addr = listen_std.local_addr().unwrap();

        let sender = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let payloads: [&[u8]; 4] = [b"a", b"bb", b"ccc", b"dddd"];
        for payload in payloads {
            sender.send_to(payload, listen_addr).await.unwrap();
        }

        let seen = Arc::new(Mutex::new(Vec::new()));
        let server = TestServer {
            name: NodeName::from_str("respond-udp").unwrap(),
            seen: seen.clone(),
        };
        let runtime = RespondUdpRuntime::new(
            server,
            UdpListenConfig::new(listen_addr),
            Arc::new(ListenStats::new(Default::default())),
            4096,
        );
        let instance = runtime.create_instance(0, listen_addr, false);
        let (reload, reload_rx) = broadcast::channel(4);
        let socket = UdpSocket::from_std(listen_std).unwrap();
        let task = tokio::spawn(instance.run(socket, reload_rx));

        for _ in payloads {
            let (body, from) = recv_from(&sender).await;
            assert_eq!(body, b"pong");
            assert_eq!(from, listen_addr);
        }
        {
            let seen = seen.lock().unwrap();
            assert_eq!(seen.len(), payloads.len());
            for (seen_pkt, payload) in seen.iter().zip(payloads) {
                assert_eq!(seen_pkt.0, payload);
                assert_eq!(seen_pkt.1, sender.local_addr().unwrap());
                assert_eq!(seen_pkt.2, listen_addr);
            }
        }

        let _ = reload.send(ServerReloadCommand::QuitRuntime);
        timeout(Duration::from_secs(2), task)
            .await
            .expect("runtime did not stop")
            .unwrap();
    }

    #[tokio::test]
    async fn reply_task_sends_each_queued_client() {
        let socket = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let from = socket.local_addr().unwrap();
        let (tx, rx) = mpsc::channel(REPLY_QUEUE);
        let listen_stats = Arc::new(ListenStats::new(Default::default()));
        let packet_recv_sender = PacketRecvSender {
            socket: Arc::new(socket),
            reply_receiver: rx,
            listen_stats: Arc::clone(&listen_stats),
        };
        let task = tokio::spawn(packet_recv_sender.run_to_end());

        let one = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let two = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let first = UdpResponder {
            client_addr: one.local_addr().unwrap(),
            server_addr: from,
            tx: tx.clone(),
        };
        let second = UdpResponder {
            client_addr: two.local_addr().unwrap(),
            server_addr: from,
            tx: tx.clone(),
        };
        let send_first = tokio::spawn(async move { first.send(b"one").await.unwrap() });
        let send_second = tokio::spawn(async move { second.send(b"two").await.unwrap() });
        send_first.await.unwrap();
        send_second.await.unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(listen_stats.datagram_send_queued(), 2);
        assert_eq!(listen_stats.datagram_send_ok(), 2);
        assert_eq!(listen_stats.datagram_send_failed(), 0);

        let (body, peer) = recv_from(&one).await;
        assert_eq!(body, b"one");
        assert_eq!(peer, from);
        let (body, peer) = recv_from(&two).await;
        assert_eq!(body, b"two");
        assert_eq!(peer, from);
    }
}
