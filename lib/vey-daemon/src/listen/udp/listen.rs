/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "macos",
    target_os = "solaris",
))]
use std::collections::VecDeque;
use std::future::poll_fn;
use std::io::{self, IoSlice, IoSliceMut};
use std::net::SocketAddr;
#[cfg(all(target_os = "linux", feature = "ebpf"))]
use std::os::fd::AsRawFd;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

#[cfg(all(target_os = "linux", feature = "ebpf"))]
use anyhow::anyhow;
use async_trait::async_trait;
use bytes::Bytes;
use foldhash::fast::FixedState;
use log::{info, warn};
use lru::LruCache;
use smallvec::SmallVec;
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};

use vey_io_ext::{UdpMoveRecv, UdpMoveSend, UdpSocketExt};
use vey_io_sys::udp::{RecvMsgHdr, SendMsgHdr};
#[cfg(all(target_os = "linux", feature = "ebpf"))]
use vey_reuseport::udp::UdpSocketSelector;
use vey_types::net::{UdpConnectionTrackConfig, UdpListenConfig};

use crate::listen::{ListenAliveGuard, ListenStats};
use crate::server::{
    BaseServer, ClientConnectionInfo, ClientConnectionKey, ReloadServer, ServerReloadCommand,
};

const EVENT_RECV_BATCH_SIZE: usize = 16;

#[derive(Default)]
struct StreamState {
    recv_dropped: AtomicUsize,
    send_dropped: AtomicUsize,
}

impl StreamState {
    fn add_recv_dropped(&self) {
        self.recv_dropped.fetch_add(1, Ordering::Relaxed);
    }

    fn add_send_dropped(&self) {
        self.send_dropped.fetch_add(1, Ordering::Relaxed);
    }
}

struct StreamDispatcher {
    state: Arc<StreamState>,
    sender: mpsc::Sender<Bytes>,
}

pub struct AcceptedUdpPacketReceiver {
    packet_max_size: u16,
    inner: mpsc::Receiver<Bytes>,
}

impl AcceptedUdpPacketReceiver {
    pub async fn recv_packet(&mut self) -> io::Result<Bytes> {
        poll_fn(|cx| self.poll_recv_packet(cx)).await
    }
}

impl UdpMoveRecv for AcceptedUdpPacketReceiver {
    type RecvError = io::Error;

    fn packet_max_size(&self) -> u16 {
        self.packet_max_size
    }

    fn poll_recv_packet(&mut self, cx: &mut Context<'_>) -> Poll<Result<Bytes, Self::RecvError>> {
        match self.inner.poll_recv(cx) {
            Poll::Ready(Some(packet)) => Poll::Ready(Ok(packet)),
            Poll::Ready(None) => Poll::Ready(Ok(Bytes::new())),
            Poll::Pending => Poll::Pending,
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
        match self.inner.poll_recv_many(cx, packets, max_count) {
            Poll::Ready(nr) => Poll::Ready(Ok(nr)),
            Poll::Pending => Poll::Pending,
        }
    }
}

enum Event {
    Packet(ClientConnectionKey, Bytes),
    Drop(ClientConnectionKey),
    Close(ClientConnectionKey),
}

type WaitPermitFuture = dyn Future<Output = Result<mpsc::OwnedPermit<Event>, mpsc::error::SendError<()>>>
    + Send
    + Sync
    + 'static;

pub struct AcceptedUdpPacketSender {
    connection_key: ClientConnectionKey,
    inner: mpsc::Sender<Event>,
    wait_permit: Option<Pin<Box<WaitPermitFuture>>>,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    batch_queue: VecDeque<Bytes>,
}

impl AcceptedUdpPacketSender {
    fn new(connection_key: ClientConnectionKey, event_sender: mpsc::Sender<Event>) -> Self {
        AcceptedUdpPacketSender {
            connection_key,
            inner: event_sender,
            wait_permit: None,
            #[cfg(any(
                target_os = "linux",
                target_os = "android",
                target_os = "freebsd",
                target_os = "netbsd",
                target_os = "openbsd",
                target_os = "macos",
                target_os = "solaris",
            ))]
            batch_queue: VecDeque::new(),
        }
    }

    pub async fn send_packet(&mut self, packet: Bytes) -> io::Result<()> {
        self.inner
            .send(Event::Packet(self.connection_key, packet))
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    pub async fn close(&mut self) {
        let _ = self.inner.send(Event::Close(self.connection_key)).await;
    }
}

impl UdpMoveSend for AcceptedUdpPacketSender {
    // TODO use never type for this
    type SendError = ();

    fn poll_send_packet(
        &mut self,
        cx: &mut Context<'_>,
        packet: &mut Option<Bytes>,
    ) -> Poll<Result<usize, Self::SendError>> {
        if packet.is_none() {
            return Poll::Ready(Ok(0));
        };

        let mut wait_permit = self
            .wait_permit
            .take()
            .unwrap_or_else(|| Box::pin(self.inner.clone().reserve_owned()));
        match wait_permit.as_mut().poll(cx) {
            Poll::Ready(Ok(permit)) => {
                let data = packet.take().unwrap();
                let data_len = data.len();
                permit.send(Event::Packet(self.connection_key, data));
                Poll::Ready(Ok(data_len))
            }
            Poll::Ready(Err(_)) => Poll::Ready(Ok(0)),
            Poll::Pending => {
                self.wait_permit = Some(wait_permit);
                Poll::Pending
            }
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
        use mpsc::error::TrySendError;

        let total_sent = packets.len();

        self.batch_queue.clear();
        self.batch_queue.reserve(packets.len());
        self.batch_queue.extend(packets.drain(..));

        let Some(first) = self.batch_queue.pop_front() else {
            return Poll::Ready(Ok(0));
        };

        let mut to_send = Some(first);
        match self.poll_send_packet(cx, &mut to_send) {
            Poll::Ready(Ok(_)) => {}
            Poll::Ready(Err(e)) => {
                if let Some(packet) = to_send {
                    packets.push(packet);
                }
                packets.extend(self.batch_queue.drain(..));
                return Poll::Ready(Err(e));
            }
            Poll::Pending => {
                if let Some(packet) = to_send {
                    packets.push(packet);
                }
                packets.extend(self.batch_queue.drain(..));
                return Poll::Pending;
            }
        }

        while let Some(packet) = self.batch_queue.pop_front() {
            match self
                .inner
                .try_send(Event::Packet(self.connection_key, packet))
            {
                Ok(_) => {}
                Err(TrySendError::Closed(Event::Packet(_, packet))) => {
                    packets.push(packet);
                    packets.extend(self.batch_queue.drain(..));
                    return Poll::Ready(Ok(total_sent - packets.len()));
                }
                Err(TrySendError::Closed(_)) => unreachable!(),
                Err(TrySendError::Full(Event::Packet(_, packet))) => {
                    packets.push(packet);
                    packets.extend(self.batch_queue.drain(..));
                    return Poll::Ready(Ok(total_sent - packets.len()));
                }
                Err(TrySendError::Full(_)) => unreachable!(),
            }
        }

        Poll::Ready(Ok(total_sent))
    }
}

#[async_trait]
pub trait AcceptUdpServer: BaseServer {
    async fn run_udp_task(
        &self,
        cc_info: ClientConnectionInfo,
        packet_receiver: AcceptedUdpPacketReceiver,
        packet_sender: AcceptedUdpPacketSender,
    );
}

struct PacketRecvSender {
    socket: Arc<UdpSocket>,
    event_receiver: mpsc::Receiver<Event>,
    global_event_sender: mpsc::Sender<Event>,
}

impl PacketRecvSender {
    async fn run_to_end(mut self) {
        let mut event_recv_buf: Vec<Event> = Vec::with_capacity(EVENT_RECV_BATCH_SIZE);

        loop {
            let limit = EVENT_RECV_BATCH_SIZE - event_recv_buf.len();
            self.event_receiver
                .recv_many(&mut event_recv_buf, limit)
                .await;
            if event_recv_buf.is_empty() {
                break;
            }
            self.handle_events(&mut event_recv_buf).await;
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
    async fn handle_events(&self, events: &mut Vec<Event>) {
        for event in events.drain(..) {
            let global_event = if let Event::Packet(key, data) = event {
                match poll_fn(|cx| {
                    let hdr = SendMsgHdr::new([IoSlice::new(&data)], Some(key.sock_peer_addr));
                    self.socket.poll_sendmsg(cx, &hdr)
                })
                .await
                {
                    Ok(_nw) => continue,
                    Err(_e) => Event::Drop(key),
                }
            } else {
                event
            };
            if self.global_event_sender.send(global_event).await.is_err() {
                break;
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
    async fn handle_events(&self, events: &mut Vec<Event>) {
        let mut data_events: SmallVec<[(ClientConnectionKey, Bytes); EVENT_RECV_BATCH_SIZE]> =
            SmallVec::new();
        let mut other_events: SmallVec<[Event; EVENT_RECV_BATCH_SIZE]> = SmallVec::new();

        for event in events.drain(..) {
            if let Event::Packet(key, data) = event {
                data_events.push((key, data));
            } else {
                other_events.push(event);
            }
        }

        while !data_events.is_empty() {
            let consumed = match poll_fn(|cx| {
                let mut headers: SmallVec<[SendMsgHdr<1>; EVENT_RECV_BATCH_SIZE]> = SmallVec::new();
                for (key, data) in &data_events {
                    headers.push(SendMsgHdr::new(
                        [IoSlice::new(data)],
                        Some(key.sock_peer_addr),
                    ))
                }
                self.socket.poll_batch_sendmsg(cx, &mut headers)
            })
            .await
            {
                Ok(n) => n,
                Err(_e) => {
                    if let Some((key, _data)) = data_events.first() {
                        let _ = self.global_event_sender.send(Event::Drop(*key)).await;
                    } else {
                        break;
                    }
                    1
                }
            };
            let _ = data_events.drain(0..consumed);
        }

        for event in other_events {
            if self.global_event_sender.send(event).await.is_err() {
                break;
            }
        }
    }
}

struct RuntimeState {
    socket: Arc<UdpSocket>,
    event_sender: mpsc::Sender<Event>,
    event_receiver: mpsc::Receiver<Event>,
}

impl RuntimeState {
    fn new(socket: UdpSocket, send_queue_size: usize) -> Self {
        let socket = Arc::new(socket);
        let (global_event_sender, global_event_receiver) = mpsc::channel(send_queue_size);
        let (event_sender, event_receiver) = mpsc::channel(send_queue_size);
        let packet_recv_sender = PacketRecvSender {
            socket: socket.clone(),
            event_receiver,
            global_event_sender,
        };
        tokio::spawn(packet_recv_sender.run_to_end());
        RuntimeState {
            socket,
            event_sender,
            event_receiver: global_event_receiver,
        }
    }
}

pub struct ListenUdpRuntime<S> {
    server: S,
    conn_track: UdpConnectionTrackConfig,
    packet_max_size: u16,
    listen_stats: Arc<ListenStats>,
    #[cfg(all(target_os = "linux", feature = "ebpf"))]
    socket_selector: Option<UdpSocketSelector>,
}

impl<S> ListenUdpRuntime<S>
where
    S: AcceptUdpServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(
        server: S,
        listen_stats: Arc<ListenStats>,
        conn_track: UdpConnectionTrackConfig,
        packet_max_size: u16,
    ) -> Self {
        ListenUdpRuntime {
            server,
            conn_track,
            packet_max_size,
            listen_stats,
            #[cfg(all(target_os = "linux", feature = "ebpf"))]
            socket_selector: None,
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
    fn create_instance(
        &self,
        id: usize,
        listen_addr: SocketAddr,
        listen_in_worker: bool,
    ) -> ListenUdpRuntimeInstance<S> {
        let server_type = self.server.r#type();
        let server_version = self.server.version();
        ListenUdpRuntimeInstance {
            server: self.server.clone(),
            server_type,
            server_version,
            worker_id: None,
            conn_track: self.conn_track,
            packet_max_size: self.packet_max_size,
            listen_stats: self.listen_stats.clone(),
            listen_addr,
            listen_in_worker,
            instance_id: id,
            _alive_guard: None,

            packet_buf: vec![0; self.packet_max_size as usize],
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
    fn create_instance(
        &self,
        id: usize,
        listen_addr: SocketAddr,
        listen_in_worker: bool,
    ) -> ListenUdpRuntimeInstance<S> {
        let server_type = self.server.r#type();
        let server_version = self.server.version();

        let mut packets_buf = Vec::with_capacity(self.conn_track.batch_recv_size());
        for _i in 0..self.conn_track.batch_recv_size() {
            packets_buf.push(vec![0; self.packet_max_size as usize]);
        }

        ListenUdpRuntimeInstance {
            server: self.server.clone(),
            server_type,
            server_version,
            worker_id: None,
            conn_track: self.conn_track,
            packet_max_size: self.packet_max_size,
            listen_stats: self.listen_stats.clone(),
            listen_addr,
            listen_in_worker,
            instance_id: id,
            _alive_guard: None,

            packets_buf,
        }
    }

    pub fn run_all_instances(
        &mut self,
        listen_config: &UdpListenConfig,
        listen_in_worker: bool,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand>,
    ) -> anyhow::Result<()> {
        let mut instance_count = listen_config.instance();
        if listen_in_worker {
            let worker_count = crate::runtime::worker::worker_count();
            if worker_count > 0 {
                instance_count = worker_count;
            }
        }

        #[cfg(all(target_os = "linux", feature = "ebpf"))]
        if listen_config.use_ebpf(rustix::process::getuid().as_raw()) {
            match UdpSocketSelector::new(
                rustix::process::getpid().as_raw_pid(),
                self.server.version() as u16,
                listen_config.address(),
                self.conn_track.ebpf_conn_track_size(),
            ) {
                Ok(selector) => {
                    self.socket_selector = Some(selector);
                }
                Err(e) => {
                    if listen_config.fail_on_ebpf_error() {
                        return Err(anyhow!(
                            "UDP {} ebpf reuseport socket selector create failed: {e}",
                            listen_config.address()
                        ));
                    }
                    warn!(
                        "reuseport ebpf on UDP socket {} disabled due to create error {e}",
                        listen_config.address()
                    );
                }
            }
        }

        for i in 0..instance_count {
            let socket = vey_socket::udp::new_std_bind_listen(listen_config)?;
            #[cfg(all(target_os = "linux", feature = "ebpf"))]
            if let Some(selector) = &mut self.socket_selector {
                selector.add_socket(socket.as_raw_fd());
            }
            let listen_addr = socket.local_addr()?;

            let runtime = self.create_instance(i, listen_addr, listen_in_worker);
            runtime.into_running(socket, server_reload_sender.subscribe());
        }

        #[cfg(all(target_os = "linux", feature = "ebpf"))]
        if let Some(mut selector) = self.socket_selector.take() {
            if let Err(e) = selector.load_and_attach() {
                if listen_config.fail_on_ebpf_error() {
                    return Err(anyhow!(
                        "UDP {} ebpf reuseport socket selector attach failed: {e}",
                        listen_config.address()
                    ));
                }
                warn!(
                    "reuseport ebpf on UDP socket {} disabled due to attach error {e}",
                    listen_config.address()
                );
            }
            let mut server_reload_receiver = server_reload_sender.subscribe();
            tokio::spawn(async move {
                while let Ok(cmd) = server_reload_receiver.recv().await {
                    if matches!(cmd, ServerReloadCommand::QuitRuntime) {
                        break;
                    }
                }
                drop(selector);
            });
        }

        Ok(())
    }
}

struct ListenUdpRuntimeInstance<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    worker_id: Option<usize>,
    conn_track: UdpConnectionTrackConfig,
    packet_max_size: u16,
    listen_stats: Arc<ListenStats>,
    listen_addr: SocketAddr,
    listen_in_worker: bool,
    instance_id: usize,
    _alive_guard: Option<ListenAliveGuard>,

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    )))]
    packet_buf: Vec<u8>,
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "solaris",
    ))]
    packets_buf: Vec<Vec<u8>>,
}

impl<S> ListenUdpRuntimeInstance<S>
where
    S: AcceptUdpServer + ReloadServer + Clone + Send + Sync + 'static,
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
        mut server_reload_channel: broadcast::Receiver<ServerReloadCommand>,
    ) {
        use broadcast::error::RecvError;

        let mut ct_table =
            LruCache::with_hasher(self.conn_track.max_sessions(), FixedState::with_seed(0));
        let mut rt_state = RuntimeState::new(socket, self.conn_track.send_queue_size());

        let mut event_recv_buf: Vec<Event> = Vec::with_capacity(EVENT_RECV_BATCH_SIZE);

        loop {
            tokio::select! {
                biased;

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
                _ = rt_state.event_receiver.recv_many(&mut event_recv_buf, EVENT_RECV_BATCH_SIZE) => {
                    // the recv number won't be zero here
                    self.handle_events(&mut event_recv_buf, &mut ct_table).await;
                    event_recv_buf.clear();
                }
                r = self.recv_packets(&rt_state.socket) => {
                    match r {
                        Ok(packets) => {
                            for (cc_info, data) in packets {
                                self.handle_packet(cc_info, data, &rt_state, &mut ct_table);
                            }
                        }
                        Err(e) => {
                            warn!("SRT[{}_v{}#{}] error receiving data from socket, error: {e}",
                                self.server.name(), self.server_version, self.instance_id);
                        }
                    }
                }
            }
        }

        #[cfg(feature = "ebpf")]
        self.run_wait_all(event_recv_buf, rt_state, ct_table).await;

        self.post_stop();
    }

    #[cfg(feature = "ebpf")]
    async fn run_wait_all(
        &mut self,
        mut event_recv_buf: Vec<Event>,
        mut rt_state: RuntimeState,
        mut ct_table: LruCache<ClientConnectionKey, StreamDispatcher, FixedState>,
    ) {
        loop {
            tokio::select! {
                biased;

                _ = rt_state.event_receiver.recv_many(&mut event_recv_buf, EVENT_RECV_BATCH_SIZE) => {
                    // the recv number won't be zero here
                    self.handle_events(&mut event_recv_buf, &mut ct_table).await;
                    event_recv_buf.clear();
                    if ct_table.is_empty() {
                        break;
                    }
                }
                r = self.recv_packets(&rt_state.socket) => {
                    match r {
                        Ok(packets) => {
                            for (cc_info, data) in packets {
                                self.handle_packet(cc_info, data, &rt_state, &mut ct_table);
                            }
                        }
                        Err(e) => {
                            warn!("SRT[{}_v{}#{}] error receiving data from socket, error: {e}",
                                self.server.name(), self.server_version, self.instance_id);
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn recv_packets(
        &mut self,
        socket: &UdpSocket,
    ) -> io::Result<SmallVec<[(ClientConnectionInfo, Bytes); EVENT_RECV_BATCH_SIZE]>> {
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
    #[allow(clippy::type_complexity)]
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context,
        socket: &UdpSocket,
    ) -> Poll<io::Result<SmallVec<[(ClientConnectionInfo, Bytes); EVENT_RECV_BATCH_SIZE]>>> {
        let mut packets = SmallVec::<[(ClientConnectionInfo, Bytes); EVENT_RECV_BATCH_SIZE]>::new();

        while packets.len() < self.conn_track.batch_recv_size() {
            let mut hdr = RecvMsgHdr::new([IoSliceMut::new(&mut self.packet_buf)]);
            match socket.poll_recvmsg(cx, &mut hdr) {
                Poll::Ready(Ok(_)) => {
                    let peer_addr = hdr
                        .src_addr()
                        .ok_or_else(|| io::Error::other("unable to get peer address"))?;
                    let local_addr = hdr.dst_addr(self.listen_addr);

                    let cc_info = ClientConnectionInfo::new(peer_addr, local_addr);
                    let nr = hdr.n_recv;
                    let data = Bytes::copy_from_slice(&self.packet_buf[..nr]);

                    packets.push((cc_info, data));
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => {
                    return if packets.is_empty() {
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(packets))
                    };
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
    #[allow(clippy::type_complexity)]
    fn poll_recv_packets(
        &mut self,
        cx: &mut Context,
        socket: &UdpSocket,
    ) -> Poll<io::Result<SmallVec<[(ClientConnectionInfo, Bytes); EVENT_RECV_BATCH_SIZE]>>> {
        let mut packets = SmallVec::<[(ClientConnectionInfo, Bytes); EVENT_RECV_BATCH_SIZE]>::new();

        let mut hdr_v = SmallVec::<[RecvMsgHdr<1>; EVENT_RECV_BATCH_SIZE]>::new();
        for p in &mut self.packets_buf {
            hdr_v.push(RecvMsgHdr::new([IoSliceMut::new(p)]))
        }

        let nr = std::task::ready!(socket.poll_batch_recvmsg(cx, &mut hdr_v))?;
        for hdr in hdr_v.iter().take(nr) {
            let peer_addr = hdr
                .src_addr()
                .ok_or_else(|| io::Error::other("unable to get peer address"))?;
            let local_addr = hdr.dst_addr(self.listen_addr);

            let cc_info = ClientConnectionInfo::new(peer_addr, local_addr);
            let nr = hdr.n_recv;
            let data = Bytes::copy_from_slice(&hdr.iov[0][..nr]);

            packets.push((cc_info, data));
        }

        Poll::Ready(Ok(packets))
    }

    fn handle_packet(
        &self,
        cc_info: ClientConnectionInfo,
        data: Bytes,
        rt_state: &RuntimeState,
        ct_table: &mut LruCache<ClientConnectionKey, StreamDispatcher, FixedState>,
    ) {
        let key = cc_info.connection_key();
        let dispatcher = ct_table.get_or_insert_ref(&key, || {
            let state = Arc::new(StreamState::default());
            let (data_sender, data_receiver) = mpsc::channel(self.conn_track.dispatch_queue_size());
            let packet_receiver = AcceptedUdpPacketReceiver {
                packet_max_size: self.packet_max_size,
                inner: data_receiver,
            };
            let packet_sender = AcceptedUdpPacketSender::new(key, rt_state.event_sender.clone());
            self.listen_stats.add_accepted();
            self.run_task(cc_info, packet_receiver, packet_sender);
            StreamDispatcher {
                state,
                sender: data_sender,
            }
        });
        match dispatcher.sender.try_send(data) {
            Ok(_) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                dispatcher.state.add_recv_dropped();
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                dispatcher.state.add_recv_dropped();
                ct_table.pop(&key);
            }
        }
    }

    fn run_task(
        &self,
        mut cc_info: ClientConnectionInfo,
        packet_receiver: AcceptedUdpPacketReceiver,
        packet_sender: AcceptedUdpPacketSender,
    ) {
        let server = self.server.clone();

        if let Some(worker_id) = self.worker_id {
            cc_info.set_worker_id(Some(worker_id));
            tokio::spawn(async move {
                server
                    .run_udp_task(cc_info, packet_receiver, packet_sender)
                    .await;
            });
            return;
        }

        if let Some(rt) = crate::runtime::worker::select_handle() {
            cc_info.set_worker_id(Some(rt.id));
            rt.handle.spawn(async move {
                server
                    .run_udp_task(cc_info, packet_receiver, packet_sender)
                    .await;
            });
        } else {
            tokio::spawn(async move {
                server
                    .run_udp_task(cc_info, packet_receiver, packet_sender)
                    .await;
            });
        }
    }

    async fn handle_events(
        &self,
        events: &mut Vec<Event>,
        ct_table: &mut LruCache<ClientConnectionKey, StreamDispatcher, FixedState>,
    ) {
        for event in events.drain(..) {
            match event {
                Event::Packet(_key, _data) => {
                    unreachable!()
                }
                Event::Drop(key) => {
                    if let Some(dispatcher) = ct_table.get(&key) {
                        dispatcher.state.add_send_dropped();
                    }
                }
                Event::Close(key) => {
                    ct_table.pop(&key);
                }
            }
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
        server_reload_channel: broadcast::Receiver<ServerReloadCommand>,
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
                        "SRT[{}_v{}#{}] udp listen async: {e:?}",
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

    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tokio::time::timeout;
    use vey_types::metrics::NodeName;

    #[derive(Debug, Clone)]
    struct SessionStart {
        session_id: usize,
        cc_info: ClientConnectionInfo,
    }

    #[derive(Debug, Clone)]
    struct ReceivedPacket {
        session_id: usize,
        data: Bytes,
    }

    #[derive(Clone)]
    struct TestServer {
        name: NodeName,
        version: usize,
        next_session_id: Arc<AtomicUsize>,
        session_starts: mpsc::UnboundedSender<SessionStart>,
        packets: mpsc::UnboundedSender<ReceivedPacket>,
        close_after_first_packet: bool,
    }

    impl BaseServer for TestServer {
        fn name(&self) -> &NodeName {
            &self.name
        }

        fn r#type(&self) -> &'static str {
            "test-udp"
        }

        fn version(&self) -> usize {
            self.version
        }
    }

    impl ReloadServer for TestServer {
        fn reload(&self) -> Self {
            let mut new_server = self.clone();
            new_server.version += 1;
            new_server
        }
    }

    #[async_trait]
    impl AcceptUdpServer for TestServer {
        async fn run_udp_task(
            &self,
            cc_info: ClientConnectionInfo,
            mut packet_receiver: AcceptedUdpPacketReceiver,
            mut packet_sender: AcceptedUdpPacketSender,
        ) {
            let session_id = self.next_session_id.fetch_add(1, Ordering::Relaxed);
            let _ = self.session_starts.send(SessionStart {
                session_id,
                cc_info: cc_info.clone(),
            });

            let mut seen_packets = 0usize;
            loop {
                match packet_receiver.recv_packet().await {
                    Ok(packet) if packet.is_empty() => break,
                    Ok(packet) => {
                        seen_packets += 1;
                        let _ = self.packets.send(ReceivedPacket {
                            session_id,
                            data: packet.clone(),
                        });
                        if self.close_after_first_packet && seen_packets == 1 {
                            packet_sender.close().await;
                            break;
                        } else {
                            let _ = packet_sender.send_packet(packet).await;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    }

    struct RuntimeHarness {
        listen_addr: SocketAddr,
        reload_sender: broadcast::Sender<ServerReloadCommand>,
        session_starts: mpsc::UnboundedReceiver<SessionStart>,
        packets: mpsc::UnboundedReceiver<ReceivedPacket>,
        runtime_task: tokio::task::JoinHandle<()>,
    }

    impl RuntimeHarness {
        async fn new(close_after_first_packet: bool) -> Self {
            let (session_starts_tx, session_starts) = mpsc::unbounded_channel();
            let (packets_tx, packets) = mpsc::unbounded_channel();

            let server = TestServer {
                name: NodeName::from_str("udp-test").unwrap(),
                version: 1,
                next_session_id: Arc::new(AtomicUsize::new(1)),
                session_starts: session_starts_tx,
                packets: packets_tx,
                close_after_first_packet,
            };

            let socket = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.set_nonblocking(true).unwrap();
            let listen_addr = socket.local_addr().unwrap();
            let socket = UdpSocket::from_std(socket).unwrap();

            let (reload_sender, reload_receiver) = broadcast::channel(4);

            let listen_stats = ListenStats::new(Default::default());
            let runtime = ListenUdpRuntime::new(
                server,
                Arc::new(listen_stats),
                UdpConnectionTrackConfig::default(),
                4096,
            )
            .create_instance(0, listen_addr, false);

            let runtime_task = tokio::spawn(runtime.run(socket, reload_receiver));

            RuntimeHarness {
                listen_addr,
                reload_sender,
                session_starts,
                packets,
                runtime_task,
            }
        }

        async fn shutdown(self) {
            let _ = self.reload_sender.send(ServerReloadCommand::QuitRuntime);
            let _ = timeout(Duration::from_secs(1), self.runtime_task).await;
        }
    }

    async fn recv_packet(socket: &UdpSocket) -> Bytes {
        let mut buf = [0u8; 64];
        let n = timeout(Duration::from_secs(1), socket.recv(&mut buf))
            .await
            .unwrap()
            .unwrap();
        Bytes::copy_from_slice(&buf[..n])
    }

    async fn recv_session_start(rx: &mut mpsc::UnboundedReceiver<SessionStart>) -> SessionStart {
        timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn recv_received_packet(
        rx: &mut mpsc::UnboundedReceiver<ReceivedPacket>,
    ) -> ReceivedPacket {
        timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap()
    }

    #[tokio::test]
    async fn routes_packets_per_connection_and_forwards_responses() {
        let mut harness = RuntimeHarness::new(false).await;

        let client_a = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_b = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        client_a
            .send_to(b"alpha-1", harness.listen_addr)
            .await
            .unwrap();
        let start_a = recv_session_start(&mut harness.session_starts).await;
        let packet_a1 = recv_received_packet(&mut harness.packets).await;
        assert_eq!(packet_a1.session_id, start_a.session_id);
        assert_eq!(packet_a1.data, Bytes::from_static(b"alpha-1"));
        assert_eq!(recv_packet(&client_a).await, Bytes::from_static(b"alpha-1"));

        client_a
            .send_to(b"alpha-2", harness.listen_addr)
            .await
            .unwrap();
        let packet_a2 = recv_received_packet(&mut harness.packets).await;
        assert_eq!(packet_a2.session_id, start_a.session_id);
        assert_eq!(packet_a2.data, Bytes::from_static(b"alpha-2"));
        assert_eq!(recv_packet(&client_a).await, Bytes::from_static(b"alpha-2"));

        client_b
            .send_to(b"bravo-1", harness.listen_addr)
            .await
            .unwrap();
        let start_b = recv_session_start(&mut harness.session_starts).await;
        let packet_b1 = recv_received_packet(&mut harness.packets).await;
        assert_ne!(start_a.session_id, start_b.session_id);
        assert_ne!(
            start_a.cc_info.connection_key(),
            start_b.cc_info.connection_key()
        );
        assert_eq!(packet_b1.session_id, start_b.session_id);
        assert_eq!(packet_b1.data, Bytes::from_static(b"bravo-1"));
        assert_eq!(recv_packet(&client_b).await, Bytes::from_static(b"bravo-1"));

        assert!(
            timeout(Duration::from_millis(100), harness.session_starts.recv())
                .await
                .is_err()
        );

        harness.shutdown().await;
    }

    #[tokio::test]
    async fn close_event_drops_connection_state_and_recreates_session() {
        let mut harness = RuntimeHarness::new(true).await;

        let client = UdpSocket::bind("127.0.0.1:0").await.unwrap();

        client.send_to(b"first", harness.listen_addr).await.unwrap();
        let start_1 = recv_session_start(&mut harness.session_starts).await;
        let packet_1 = recv_received_packet(&mut harness.packets).await;
        assert_eq!(packet_1.session_id, start_1.session_id);
        assert_eq!(packet_1.data, Bytes::from_static(b"first"));

        client
            .send_to(b"second", harness.listen_addr)
            .await
            .unwrap();
        let start_2 = recv_session_start(&mut harness.session_starts).await;
        let packet_2 = recv_received_packet(&mut harness.packets).await;
        assert_ne!(start_1.session_id, start_2.session_id);
        assert_eq!(
            start_1.cc_info.connection_key(),
            start_2.cc_info.connection_key()
        );
        assert_eq!(packet_2.session_id, start_2.session_id);
        assert_eq!(packet_2.data, Bytes::from_static(b"second"));

        harness.shutdown().await;
    }
}
