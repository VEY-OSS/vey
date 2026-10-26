/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! One UDP datagram in, one datagram back.
//!
//! [`ReceiveUdpRuntime`] only delivers inbound packets. This runtime is for a
//! protocol whose request and response each fit in a single datagram.
//! [`UdpResponder`] is that one reply: it already names its client. The socket
//! task collects those replies and submits them with `sendmmsg`, the same way
//! `ListenUdpRuntime` batches `SendMsgHdr`s.

use std::future::{Future, poll_fn};
use std::io::{self, IoSlice, IoSliceMut};
use std::net::SocketAddr;
use std::sync::Arc;

#[cfg(feature = "ebpf")]
use anyhow::anyhow;
use log::{info, warn};
use smallvec::SmallVec;
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use vey_io_ext::UdpSocketExt;
use vey_io_sys::udp::{RecvMsgHdr, SendMsgHdr};
#[cfg(feature = "ebpf")]
use vey_reuseport::udp::{UdpSocketSelectGuard, UdpSocketSelector};
#[cfg(feature = "ebpf")]
use vey_socket::RawSocket;
#[cfg(feature = "ebpf")]
use vey_types::net::UdpConnectionTrackConfig;
use vey_types::net::UdpListenConfig;

use crate::server::{BaseServer, ReloadServer, ServerReloadCommand};

const REPLY_QUEUE: usize = 512;
const REPLY_BATCH: usize = 16;

struct PendingReply {
    client_addr: SocketAddr,
    packet: Vec<u8>,
    done: oneshot::Sender<io::Result<()>>,
}

/// One response to one client.
///
/// The socket task batches every reply queued on this listen socket. This
/// value only carries the destination for its own packet.
#[derive(Clone)]
pub struct UdpResponder {
    client_addr: SocketAddr,
    tx: mpsc::Sender<PendingReply>,
}

impl UdpResponder {
    pub fn client_addr(&self) -> SocketAddr {
        self.client_addr
    }

    /// Same listen socket, different destination.
    pub fn to(&self, client_addr: SocketAddr) -> Self {
        UdpResponder {
            client_addr,
            tx: self.tx.clone(),
        }
    }

    /// Queue this one packet and wait until the socket task has written it.
    pub async fn send(&self, packet: &[u8]) -> io::Result<()> {
        let (done, result) = oneshot::channel();
        self.tx
            .send(PendingReply {
                client_addr: self.client_addr,
                packet: packet.to_vec(),
                done,
            })
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "responder closed"))?;
        result
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "responder closed"))?
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
        client_addr: SocketAddr,
        server_addr: SocketAddr,
        responder: UdpResponder,
        worker_id: Option<usize>,
    ) -> impl Future<Output = ()> + Send + 'a;
}

/// A running responder task and the address its socket is bound to.
pub struct RespondUdpTask {
    pub local_addr: SocketAddr,
    pub handle: JoinHandle<()>,
}

/// Receives one request datagram at a time and hands the caller a [`UdpResponder`].
///
/// On Linux with the `ebpf` feature, `run_all_instances` attaches the reuseport
/// selector used for a lossless upgrade. The new process steers new clients to
/// itself. Clients already stuck to a live socket stay there until that socket
/// leaves the map, and the kernel then hands the same packet to a live socket.
pub struct RespondUdpRuntime<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    worker_id: Option<usize>,
    listen_config: UdpListenConfig,
    instance_id: usize,
    #[cfg(feature = "ebpf")]
    socket_selector: Option<UdpSocketSelector>,
    #[cfg(feature = "ebpf")]
    _bpf_guard: Option<UdpSocketSelectGuard>,
}

impl<S> RespondUdpRuntime<S>
where
    S: RespondUdpServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(server: S, listen_config: UdpListenConfig) -> Self {
        let server_type = server.r#type();
        let server_version = server.version();
        RespondUdpRuntime {
            server,
            server_type,
            server_version,
            worker_id: None,
            listen_config,
            instance_id: 0,
            #[cfg(feature = "ebpf")]
            socket_selector: None,
            #[cfg(feature = "ebpf")]
            _bpf_guard: None,
        }
    }

    fn instance(&self, instance_id: usize) -> Self {
        RespondUdpRuntime {
            server: self.server.clone(),
            server_type: self.server_type,
            server_version: self.server_version,
            worker_id: None,
            listen_config: self.listen_config.clone(),
            instance_id,
            #[cfg(feature = "ebpf")]
            socket_selector: None,
            #[cfg(feature = "ebpf")]
            _bpf_guard: None,
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
        listen_addr: SocketAddr,
        mut server_reload_channel: broadcast::Receiver<ServerReloadCommand<()>>,
    ) {
        use broadcast::error::RecvError;

        let socket = Arc::new(socket);
        let (reply_tx, reply_rx) = mpsc::channel(REPLY_QUEUE);
        let reply_task = tokio::spawn(run_replies(Arc::clone(&socket), reply_rx));
        let mut buf = [0u8; u16::MAX as usize];
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
                r = recv_packet(&socket, listen_addr, &mut buf) => {
                    match r {
                        Ok((len, client_addr, server_addr)) => {
                            let responder = UdpResponder {
                                client_addr,
                                tx: reply_tx.clone(),
                            };
                            self.server
                                .handle_udp_request(
                                    &buf[..len],
                                    client_addr,
                                    server_addr,
                                    responder,
                                    self.worker_id,
                                )
                                .await;
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
        self.drain_after_reuseport_detach(&socket, &reply_tx, listen_addr, &mut buf)
            .await;

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
        listen_addr: SocketAddr,
        buf: &mut [u8],
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
                r = recv_packet(socket, listen_addr, buf) => {
                    match r {
                        Ok((len, client_addr, server_addr)) => {
                            let responder = UdpResponder {
                                client_addr,
                                tx: reply_tx.clone(),
                            };
                            self.server
                                .handle_udp_request(
                                    &buf[..len],
                                    client_addr,
                                    server_addr,
                                    responder,
                                    self.worker_id,
                                )
                                .await;
                        }
                        Err(_) => break,
                    }
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
        server_reload_channel: broadcast::Receiver<ServerReloadCommand<()>>,
    ) -> JoinHandle<()> {
        let handle = self.get_rt_handle(listen_in_worker);
        handle.spawn(async move {
            match UdpSocket::from_std(socket) {
                Ok(socket) => {
                    self.pre_start();
                    self.run(socket, listen_addr, server_reload_channel).await;
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
        })
    }

    fn instance_count(&self, listen_in_worker: bool) -> usize {
        let mut instance_count = self.listen_config.instance();
        if listen_in_worker {
            let worker_count = crate::runtime::worker::worker_count();
            if worker_count > 0 {
                instance_count = worker_count;
            }
        }
        instance_count
    }

    /// Bind one socket per instance and run a responder task on each.
    ///
    /// `listen_in_worker` places one instance on every worker when workers
    /// exist. Each task has its own socket; reply on the [`UdpResponder`]
    /// delivered to that task so the response leaves the socket that received
    /// the request.
    #[cfg(not(feature = "ebpf"))]
    pub fn run_all_instances(
        &mut self,
        listen_in_worker: bool,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand<()>>,
    ) -> anyhow::Result<Vec<RespondUdpTask>> {
        let instance_count = self.instance_count(listen_in_worker);
        let mut tasks = Vec::with_capacity(instance_count);
        for i in 0..instance_count {
            let socket = vey_socket::udp::new_std_bind_listen(&self.listen_config)?;
            let listen_addr = socket.local_addr()?;
            let runtime = self.instance(i);
            let handle = runtime.into_running(
                socket,
                listen_addr,
                listen_in_worker,
                server_reload_sender.subscribe(),
            );
            tasks.push(RespondUdpTask {
                local_addr: listen_addr,
                handle,
            });
        }
        Ok(tasks)
    }

    /// Bind one socket per instance and run a responder task on each.
    ///
    /// `listen_in_worker` places one instance on every worker when workers
    /// exist. Each task has its own socket; reply on the [`UdpResponder`]
    /// delivered to that task so the response leaves the socket that received
    /// the request.
    ///
    /// Every socket joins the reuseport selector before the program is attached.
    /// A newer process that attaches the same address receives new clients.
    /// This process leaves the selector on `QuitRuntime`.
    #[cfg(feature = "ebpf")]
    pub fn run_all_instances(
        &mut self,
        listen_in_worker: bool,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand<()>>,
    ) -> anyhow::Result<Vec<RespondUdpTask>> {
        let instance_count = self.instance_count(listen_in_worker);

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

        let mut tasks = Vec::with_capacity(instance_count);
        for i in 0..instance_count {
            let socket = vey_socket::udp::new_std_bind_listen(&self.listen_config)?;
            let listen_addr = socket.local_addr()?;

            let mut runtime = self.instance(i);
            if let Some(selector) = &mut self.socket_selector {
                let guard = selector.add_socket(RawSocket::from(&socket));
                runtime._bpf_guard = Some(guard);
            }
            let handle = runtime.into_running(
                socket,
                listen_addr,
                listen_in_worker,
                server_reload_sender.subscribe(),
            );
            tasks.push(RespondUdpTask {
                local_addr: listen_addr,
                handle,
            });
        }

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

        Ok(tasks)
    }
}

async fn run_replies(socket: Arc<UdpSocket>, mut rx: mpsc::Receiver<PendingReply>) {
    let mut queued = Vec::with_capacity(REPLY_BATCH);
    loop {
        let n = rx.recv_many(&mut queued, REPLY_BATCH).await;
        if n == 0 {
            break;
        }
        flush_replies(&socket, &mut queued).await;
    }
}

fn reply_hdr(reply: &PendingReply) -> SendMsgHdr<'_, 1> {
    SendMsgHdr::new(
        [IoSlice::new(reply.packet.as_slice())],
        Some(reply.client_addr),
    )
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "solaris",
)))]
async fn flush_replies(socket: &UdpSocket, queued: &mut Vec<PendingReply>) {
    for reply in queued.drain(..) {
        let result = match poll_fn(|cx| {
            let hdr = reply_hdr(&reply);
            socket.poll_sendmsg(cx, &hdr)
        })
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        };
        finish_reply(reply, result);
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
async fn flush_replies(socket: &UdpSocket, queued: &mut Vec<PendingReply>) {
    while !queued.is_empty() {
        let consumed = match poll_fn(|cx| {
            let mut headers: SmallVec<[SendMsgHdr<1>; REPLY_BATCH]> = SmallVec::new();
            for reply in queued.iter() {
                headers.push(reply_hdr(reply));
            }
            socket.poll_batch_sendmsg(cx, &mut headers)
        })
        .await
        {
            Ok(n) => n,
            Err(e) => {
                if queued.first().is_some() {
                    let reply = queued.remove(0);
                    finish_reply(reply, Err(e));
                } else {
                    break;
                }
                continue;
            }
        };
        for reply in queued.drain(0..consumed) {
            finish_reply(reply, Ok(()));
        }
    }
}

fn finish_reply(reply: PendingReply, result: io::Result<()>) {
    let _ = reply.done.send(result);
}

async fn recv_packet(
    socket: &UdpSocket,
    listen_addr: SocketAddr,
    buf: &mut [u8],
) -> io::Result<(usize, SocketAddr, SocketAddr)> {
    let mut hdr = RecvMsgHdr::new([IoSliceMut::new(buf)]);

    poll_fn(|cx| socket.poll_recvmsg(cx, &mut hdr)).await?;

    let client_addr = hdr
        .src_addr()
        .ok_or_else(|| io::Error::other("unable to get client address"))?;
    let server_addr = hdr.dst_addr(listen_addr);

    Ok((hdr.n_recv, client_addr, server_addr))
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

    #[derive(Clone)]
    struct TestServer {
        name: NodeName,
        reply_to: Arc<Mutex<Option<SocketAddr>>>,
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
            client_addr: SocketAddr,
            server_addr: SocketAddr,
            responder: UdpResponder,
            worker_id: Option<usize>,
        ) -> impl Future<Output = ()> + Send + 'a {
            let dest = self.reply_to.lock().unwrap().unwrap_or(client_addr);
            async move {
                let _ = (packet, server_addr, worker_id);
                responder.to(dest).send(b"pong").await.unwrap();
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
        let reply_to = Arc::new(Mutex::new(None));
        let server = TestServer {
            name: NodeName::from_str("respond-udp").unwrap(),
            reply_to: reply_to.clone(),
        };
        let listen = UdpListenConfig::new("127.0.0.1:0".parse().unwrap());
        let mut runtime = RespondUdpRuntime::new(server, listen);
        let reload = ServerReloadCommand::new_sender();
        let mut tasks = runtime.run_all_instances(false, &reload).unwrap();
        let task = tasks.pop().unwrap();
        assert!(tasks.is_empty());

        let sender = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let other = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        *reply_to.lock().unwrap() = Some(other.local_addr().unwrap());

        sender.send_to(b"ping", task.local_addr).await.unwrap();

        let (body, from) = recv_from(&other).await;
        assert_eq!(body, b"pong");
        assert_eq!(from, task.local_addr);

        let mut buf = [0u8; 16];
        let missed = timeout(Duration::from_millis(200), sender.recv_from(&mut buf)).await;
        assert!(
            missed.is_err(),
            "sender should not receive a reply aimed at other"
        );

        let _ = reload.send(ServerReloadCommand::QuitRuntime);
        timeout(Duration::from_secs(2), task.handle)
            .await
            .expect("runtime did not stop")
            .unwrap();
    }

    #[tokio::test]
    async fn reply_task_sends_each_queued_client() {
        let socket = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let from = socket.local_addr().unwrap();
        let (tx, rx) = mpsc::channel(REPLY_QUEUE);
        let task = tokio::spawn(run_replies(Arc::new(socket), rx));

        let one = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let two = TokioUdpSocket::bind("127.0.0.1:0").await.unwrap();
        let first = UdpResponder {
            client_addr: one.local_addr().unwrap(),
            tx: tx.clone(),
        };
        let second = UdpResponder {
            client_addr: two.local_addr().unwrap(),
            tx: tx.clone(),
        };
        let send_first = tokio::spawn(async move { first.send(b"one").await.unwrap() });
        let send_second = tokio::spawn(async move { second.send(b"two").await.unwrap() });
        send_first.await.unwrap();
        send_second.await.unwrap();
        drop(tx);
        task.await.unwrap();

        let (body, peer) = recv_from(&one).await;
        assert_eq!(body, b"one");
        assert_eq!(peer, from);
        let (body, peer) = recv_from(&two).await;
        assert_eq!(body, b"two");
        assert_eq!(peer, from);
    }
}
