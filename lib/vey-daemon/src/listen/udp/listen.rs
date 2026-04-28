/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;

use bytes::Bytes;
use foldhash::fast::FixedState;
use log::{info, warn};
use lru::LruCache;
use tokio::net::UdpSocket;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};

use vey_io_ext::UdpSocketExt;
use vey_io_sys::udp::RecvMsgHdr;
use vey_types::net::{UdpConnectionTrackConfig, UdpListenConfig};

use crate::server::{
    BaseServer, ClientConnectionInfo, ClientConnectionKey, ReloadServer, ServerReloadCommand,
};

const CLOSE_RECV_BATCH_SIZE: usize = 16;

pub trait AcceptUdpServer: BaseServer {
    fn run_udp_task(
        &self,
        cc_info: ClientConnectionInfo,
        packet_receiver: mpsc::Receiver<Bytes>,
        close_notifier: mpsc::Sender<ClientConnectionKey>,
    );
}

#[derive(Clone)]
pub struct ListenUdpRuntime<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    worker_id: Option<usize>,
    listen_config: UdpListenConfig,
    conn_track: UdpConnectionTrackConfig,
    //listen_stats: Arc<ListenStats>,
    instance_id: usize,
}

impl<S> ListenUdpRuntime<S>
where
    S: AcceptUdpServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(
        server: S,
        listen_config: UdpListenConfig,
        conn_track: UdpConnectionTrackConfig,
    ) -> Self {
        let server_type = server.r#type();
        let server_version = server.version();
        ListenUdpRuntime {
            server,
            server_type,
            server_version,
            worker_id: None,
            listen_config,
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
        let (close_sender, mut close_receiver) = mpsc::channel(self.conn_track.close_queue_size());

        let mut close_recv_buf: Vec<ClientConnectionKey> =
            Vec::with_capacity(CLOSE_RECV_BATCH_SIZE);

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
                n = close_receiver.recv_many(&mut close_recv_buf, CLOSE_RECV_BATCH_SIZE) => {
                    for key in &close_recv_buf[0..n] {
                        connection_table.pop(key);
                    }
                }
                r = self.recv_packet(&socket, listen_addr, &mut buf) => {
                    match r {
                        Ok((cc_info, data)) => {
                            let key = cc_info.connection_key();
                            let sender = connection_table.get_or_insert_ref(&key, || {
                                let (data_sender, data_receiver) = mpsc::channel(self.conn_track.session_queue_size());
                                self.server.run_udp_task(cc_info, data_receiver, close_sender.clone());
                                data_sender
                            });
                            match sender.try_send(data) {
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

        let mut cc_info = ClientConnectionInfo::new(peer_addr, local_addr);
        cc_info.set_worker_id(self.worker_id);

        let nr = hdr.n_recv;
        let data = Bytes::copy_from_slice(&buf[..nr]);

        Ok((cc_info, data))
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
        listen_in_worker: bool,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand>,
    ) -> anyhow::Result<()> {
        let mut instance_count = self.listen_config.instance();
        if listen_in_worker {
            let worker_count = crate::runtime::worker::worker_count();
            if worker_count > 0 {
                instance_count = worker_count;
            }
        }

        for i in 0..instance_count {
            let mut runtime = self.clone();
            runtime.instance_id = i;

            let socket = vey_socket::udp::new_std_bind_listen(&self.listen_config)?;
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
