/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::io::{self, IoSlice, IoSliceMut};
use std::net::SocketAddr;
use std::task::{Context, Poll};

use async_trait::async_trait;
use bytes::Bytes;
use foldhash::fast::FixedState;
use log::{info, warn};
use lru::LruCache;
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};

use vey_io_ext::UdpSocketExt;
use vey_io_sys::udp::{RecvMsgHdr, SendMsgHdr};
use vey_types::net::{UdpConnectionTrackConfig, UdpListenConfig};

use crate::server::{
    BaseServer, ClientConnectionInfo, ClientConnectionKey, ReloadServer, ServerReloadCommand,
};

const EVENT_RECV_BATCH_SIZE: usize = 16;

pub struct AcceptedUdpPacketReceiver {
    inner: mpsc::Receiver<io::Result<Bytes>>,
}

impl AcceptedUdpPacketReceiver {
    pub async fn recv_packet(&mut self) -> io::Result<Bytes> {
        self.inner.recv().await.unwrap_or(Ok(Bytes::new()))
    }

    pub fn poll_recv_packet(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<Bytes>> {
        match self.inner.poll_recv(cx) {
            Poll::Ready(Some(packet)) => Poll::Ready(packet),
            Poll::Ready(None) => Poll::Ready(Ok(Bytes::new())),
            Poll::Pending => Poll::Pending,
        }
    }
}

enum Event {
    Packet(ClientConnectionKey, Bytes),
    Close(ClientConnectionKey),
}

pub struct AcceptedUdpPacketSender {
    connection_key: ClientConnectionKey,
    inner: mpsc::UnboundedSender<Event>,
}

impl AcceptedUdpPacketSender {
    pub fn send_packet(&mut self, packet: Bytes) -> io::Result<()> {
        self.inner
            .send(Event::Packet(self.connection_key, packet))
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
    }

    pub fn close(&mut self) -> io::Result<()> {
        self.inner
            .send(Event::Close(self.connection_key))
            .map_err(|e| io::Error::new(io::ErrorKind::BrokenPipe, e))
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

#[derive(Clone)]
pub struct ListenUdpRuntime<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    worker_id: Option<usize>,
    conn_track: UdpConnectionTrackConfig,
    //listen_stats: Arc<ListenStats>,
    instance_id: usize,
}

impl<S> ListenUdpRuntime<S>
where
    S: AcceptUdpServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(server: S, conn_track: UdpConnectionTrackConfig) -> Self {
        let server_type = server.r#type();
        let server_version = server.version();
        ListenUdpRuntime {
            server,
            server_type,
            server_version,
            worker_id: None,
            conn_track,
            instance_id: 0,
        }
    }

    fn pre_start(&self) {
        info!(
            "started {} SRT[{}_v{}#{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
            self.instance_id,
        );
        //self.listen_stats.add_running_runtime();
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
        //self.listen_stats.del_running_runtime();
    }

    async fn run(
        mut self,
        socket: UdpSocket,
        listen_addr: SocketAddr,
        mut server_reload_channel: broadcast::Receiver<ServerReloadCommand>,
    ) {
        use broadcast::error::RecvError;

        let mut connection_table =
            LruCache::with_hasher(self.conn_track.max_sessions(), FixedState::with_seed(0));
        let (event_sender, mut event_receiver) = mpsc::unbounded_channel();

        let mut event_recv_buf: Vec<Event> = Vec::with_capacity(EVENT_RECV_BATCH_SIZE);

        let mut buf = [0u8; u16::MAX as usize];
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
                n = event_receiver.recv_many(&mut event_recv_buf, EVENT_RECV_BATCH_SIZE) => {
                    self.handle_events(&socket, &event_recv_buf[..n], &mut connection_table).await;
                    event_recv_buf.clear();
                }
                r = self.recv_packet(&socket, listen_addr, &mut buf) => {
                    match r {
                        Ok((cc_info, data)) => {
                            let key = cc_info.connection_key();
                            let sender = connection_table.get_or_insert_ref(&key, || {
                                let (data_sender, data_receiver) = mpsc::channel(self.conn_track.session_queue_size());
                                let packet_receiver = AcceptedUdpPacketReceiver {
                                    inner: data_receiver,
                                };
                                let packet_sender = AcceptedUdpPacketSender {
                                    connection_key: key,
                                    inner: event_sender.clone(),
                                };
                                self.run_task(cc_info, packet_receiver, packet_sender);
                                data_sender
                            });
                            match sender.try_send(Ok(data)) {
                                Ok(_) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    // TODO record dropped data
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    // TODO record dropped data
                                    connection_table.pop(&key);
                                }
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

        self.post_stop();
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

    async fn recv_packet(
        &self,
        socket: &UdpSocket,
        listen_addr: SocketAddr,
        buf: &mut [u8],
    ) -> io::Result<(ClientConnectionInfo, Bytes)> {
        let mut hdr = RecvMsgHdr::new([IoSliceMut::new(buf)]);

        poll_fn(|cx| socket.poll_recvmsg(cx, &mut hdr)).await?;

        let peer_addr = hdr
            .src_addr()
            .ok_or_else(|| io::Error::other("unable to get peer address"))?;
        let local_addr = hdr.dst_addr(listen_addr);

        let cc_info = ClientConnectionInfo::new(peer_addr, local_addr);

        let nr = hdr.n_recv;
        let data = Bytes::copy_from_slice(&buf[..nr]);

        Ok((cc_info, data))
    }

    async fn handle_events(
        &self,
        socket: &UdpSocket,
        events: &[Event],
        connection_table: &mut LruCache<
            ClientConnectionKey,
            mpsc::Sender<io::Result<Bytes>>,
            FixedState,
        >,
    ) {
        for event in events {
            match event {
                Event::Packet(key, data) => {
                    let hdr = SendMsgHdr::new([IoSlice::new(data)], Some(key.sock_peer_addr));
                    match poll_fn(move |cx| socket.poll_sendmsg(cx, &hdr)).await {
                        Ok(_nw) => {}
                        Err(e) => {
                            if let Some(sender) = connection_table.get(key) {
                                let _ = sender.send(Err(e)).await;
                            }
                        }
                    }
                }
                Event::Close(key) => {
                    connection_table.pop(key);
                }
            }
        }
    }

    fn get_rt_handle(&mut self, listen_in_worker: bool) -> Handle {
        if listen_in_worker && let Some(rt) = crate::runtime::worker::select_listen_handle() {
            self.worker_id = Some(rt.id);
            return rt.handle;
        }
        Handle::current()
    }

    fn into_running(
        mut self,
        socket: std::net::UdpSocket,
        listen_addr: SocketAddr,
        listen_in_worker: bool,
        server_reload_channel: broadcast::Receiver<ServerReloadCommand>,
    ) {
        let handle = self.get_rt_handle(listen_in_worker);
        handle.spawn(async move {
            // make sure the listen socket associated with the correct reactor
            match UdpSocket::from_std(socket) {
                Ok(socket) => {
                    self.pre_start();
                    self.run(socket, listen_addr, server_reload_channel).await;
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

    pub fn run_all_instances(
        &self,
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

        for i in 0..instance_count {
            let mut runtime = self.clone();
            runtime.instance_id = i;

            let socket = vey_socket::udp::new_std_bind_listen(listen_config)?;
            let listen_addr = socket.local_addr()?;
            runtime.into_running(
                socket,
                listen_addr,
                listen_in_worker,
                server_reload_sender.subscribe(),
            );
        }
        Ok(())
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
                            let _ = packet_sender.close();
                            break;
                        } else {
                            let _ = packet_sender.send_packet(packet);
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

            let runtime = ListenUdpRuntime::new(server, UdpConnectionTrackConfig::default());

            let runtime_task = tokio::spawn(runtime.run(socket, listen_addr, reload_receiver));

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
