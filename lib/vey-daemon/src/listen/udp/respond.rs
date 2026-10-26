/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! One UDP datagram in, one datagram back.
//!
//! [`ReceiveUdpRuntime`] only delivers inbound packets. This runtime is for a
//! protocol whose request and response each fit in a single datagram.
//! [`UdpResponder::send_to`] names the destination client address, so the
//! reply can leave immediately or later from another task, and it can be
//! aimed at a different socket than the one that sent the request.

use std::future::Future;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::sync::Arc;

#[cfg(feature = "ebpf")]
use anyhow::anyhow;
use log::{info, warn};
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use vey_io_ext::UdpSocketExt;
use vey_io_sys::udp::RecvMsgHdr;
#[cfg(feature = "ebpf")]
use vey_reuseport::udp::{UdpSocketSelectGuard, UdpSocketSelector};
#[cfg(feature = "ebpf")]
use vey_socket::RawSocket;
#[cfg(feature = "ebpf")]
use vey_types::net::UdpConnectionTrackConfig;
use vey_types::net::UdpListenConfig;

use crate::server::{BaseServer, ReloadServer, ServerReloadCommand};

/// Sends one response datagram to an explicit client address.
///
/// Cloning is cheap. Hold a clone next to the request when the response is
/// produced somewhere else; pass that request's client address to [`send_to`](Self::send_to).
#[derive(Clone)]
pub struct UdpResponder {
    socket: Arc<UdpSocket>,
}

impl UdpResponder {
    /// Send `packet` to `client_addr`.
    ///
    /// `client_addr` is the destination. It is not remembered from the last
    /// receive.
    pub async fn send_to(&self, packet: &[u8], client_addr: SocketAddr) -> io::Result<()> {
        let n = self.socket.send_to(packet, client_addr).await?;
        if n != packet.len() {
            return Err(io::Error::other(format!(
                "not all data written, only {n}/{}",
                packet.len()
            )));
        }
        Ok(())
    }
}

/// Handles one request datagram.
///
/// The runtime does not send a response. Call [`UdpResponder::send_to`] with
/// the client address that should receive it. `client_addr` is the source of
/// this packet; use it as the destination for a direct reply.
pub trait RespondUdpServer: BaseServer {
    fn handle_udp_request<'a>(
        &'a self,
        packet: &'a [u8],
        client_addr: SocketAddr,
        server_addr: SocketAddr,
        responder: &'a UdpResponder,
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

        let responder = UdpResponder {
            socket: Arc::new(socket),
        };
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
                r = recv_packet(&responder.socket, listen_addr, &mut buf) => {
                    match r {
                        Ok((len, client_addr, server_addr)) => {
                            self.server
                                .handle_udp_request(
                                    &buf[..len],
                                    client_addr,
                                    server_addr,
                                    &responder,
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
        self.drain_after_reuseport_detach(&responder, listen_addr, &mut buf)
            .await;

        self.post_stop();
    }

    /// Drop this process out of the reuseport map, then read datagrams already
    /// queued on the socket so closing the fd does not discard them.
    #[cfg(feature = "ebpf")]
    async fn drain_after_reuseport_detach(
        &mut self,
        responder: &UdpResponder,
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
                r = recv_packet(&responder.socket, listen_addr, buf) => {
                    match r {
                        Ok((len, client_addr, server_addr)) => {
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

async fn recv_packet(
    socket: &UdpSocket,
    listen_addr: SocketAddr,
    buf: &mut [u8],
) -> io::Result<(usize, SocketAddr, SocketAddr)> {
    let mut hdr = RecvMsgHdr::new([IoSliceMut::new(buf)]);

    std::future::poll_fn(|cx| socket.poll_recvmsg(cx, &mut hdr)).await?;

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
            responder: &'a UdpResponder,
            worker_id: Option<usize>,
        ) -> impl Future<Output = ()> + Send + 'a {
            let dest = self.reply_to.lock().unwrap().unwrap_or(client_addr);
            async move {
                let _ = (packet, server_addr, worker_id);
                responder.send_to(b"pong", dest).await.unwrap();
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
}
