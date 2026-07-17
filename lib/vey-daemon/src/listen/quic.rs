/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
use anyhow::anyhow;
use async_trait::async_trait;
use log::{info, warn};
use quinn::{Connection, Endpoint, Incoming};
use tokio::runtime::Handle;
use tokio::sync::broadcast;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
use vey_reuseport::quic::{QuicSocketSelectGuard, QuicSocketSelector};
use vey_socket::RawSocket;
use vey_std_ext::net::SocketAddrExt;
use vey_types::acl::{AclAction, AclNetworkRule};
use vey_types::net::UdpListenConfig;

use crate::listen::{ListenAliveGuard, ListenStats};
use crate::server::{BaseServer, ClientConnectionInfo, ReloadServer, ServerReloadCommand};

#[async_trait]
pub trait AcceptQuicServer: BaseServer {
    async fn run_quic_task(&self, connection: Connection, cc_info: ClientConnectionInfo);
}

#[derive(Clone)]
pub enum ListenQuicInPlaceConfig {
    ListenConfig(UdpListenConfig),
    QuinnConfig(quinn::ServerConfig),
    IngressAcl(Option<Arc<AclNetworkRule>>),
    AcceptTimeout(Duration),
}

pub struct ListenQuicRuntime<S> {
    server: S,
    listen_config: UdpListenConfig,
    listen_stats: Arc<ListenStats>,
    #[cfg(all(target_os = "linux", feature = "ebpf"))]
    socket_selector: Option<QuicSocketSelector>,
}

impl<S> ListenQuicRuntime<S>
where
    S: AcceptQuicServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(server: S, listen_stats: Arc<ListenStats>, listen_config: UdpListenConfig) -> Self {
        ListenQuicRuntime {
            server,
            listen_config,
            listen_stats,
            #[cfg(all(target_os = "linux", feature = "ebpf"))]
            socket_selector: None,
        }
    }

    pub fn run_all_instances(
        &mut self,
        listen_in_worker: bool,
        quic_config: &quinn::ServerConfig,
        ingress_net_filter: Option<&Arc<AclNetworkRule>>,
        accept_timeout: Duration,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand<ListenQuicInPlaceConfig>>,
    ) -> anyhow::Result<()> {
        let mut instance_count = self.listen_config.instance();
        if listen_in_worker {
            let worker_count = crate::runtime::worker::worker_count();
            if worker_count > 0 {
                instance_count = worker_count;
            }
        }

        #[cfg(all(target_os = "linux", feature = "ebpf"))]
        if self
            .listen_config
            .use_ebpf(rustix::process::getuid().as_raw())
        {
            match QuicSocketSelector::new(
                rustix::process::getpid().as_raw_pid(),
                self.server.version() as u16,
                self.listen_config.address(),
            ) {
                Ok(selector) => {
                    self.socket_selector = Some(selector);
                }
                Err(e) => {
                    if self.listen_config.fail_on_ebpf_error() {
                        return Err(anyhow!(
                            "QUIC {} ebpf reuseport socket selector create failed: {e}",
                            self.listen_config.address()
                        ));
                    }
                    warn!(
                        "reuseport ebpf on QUIC socket {} disabled due to create error {e}",
                        self.listen_config.address()
                    );
                }
            }
        }

        for i in 0..instance_count {
            let socket = vey_socket::udp::new_std_bind_listen(&self.listen_config)?;
            #[cfg(all(target_os = "linux", feature = "ebpf"))]
            let guard = if let Some(selector) = &mut self.socket_selector {
                let guard = selector.add_socket(RawSocket::from(&socket))?;
                Some(guard)
            } else {
                None
            };
            let listen_addr = socket.local_addr()?;

            let runtime = ListenQuicRuntimeInstance {
                server: self.server.clone(),
                server_type: self.server.r#type(),
                server_version: self.server.version(),
                worker_id: None,
                listen_config: self.listen_config.clone(),
                listen_stats: self.listen_stats.clone(),
                listen_addr,
                listen_in_worker,
                instance_id: i,
                ingress_net_filter: ingress_net_filter.cloned(),
                accept_timeout,
                #[cfg(all(target_os = "linux", feature = "ebpf"))]
                _bpf_guard: guard,
                _alive_guard: None,
            };
            runtime.into_running(
                socket,
                quic_config.clone(),
                server_reload_sender.subscribe(),
            );
        }

        #[cfg(all(target_os = "linux", feature = "ebpf"))]
        if let Some(mut selector) = self.socket_selector.take() {
            if let Err(e) = selector.load_and_attach() {
                if self.listen_config.fail_on_ebpf_error() {
                    return Err(anyhow!(
                        "QUIC {} ebpf reuseport socket selector attach failed: {e}",
                        self.listen_config.address()
                    ));
                }
                warn!(
                    "reuseport ebpf on QUIC socket {} disabled due to attach error {e}",
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

pub struct ListenQuicRuntimeInstance<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    worker_id: Option<usize>,
    listen_config: UdpListenConfig,
    listen_stats: Arc<ListenStats>,
    listen_addr: SocketAddr,
    listen_in_worker: bool,
    instance_id: usize,
    ingress_net_filter: Option<Arc<AclNetworkRule>>,
    accept_timeout: Duration,
    #[cfg(all(target_os = "linux", feature = "ebpf"))]
    _bpf_guard: Option<QuicSocketSelectGuard>,
    _alive_guard: Option<ListenAliveGuard>,
}

impl<S> ListenQuicRuntimeInstance<S>
where
    S: AcceptQuicServer + ReloadServer + Clone + Send + Sync + 'static,
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
        listener: Endpoint,
        raw_socket: RawSocket,
        mut server_reload_channel: broadcast::Receiver<
            ServerReloadCommand<ListenQuicInPlaceConfig>,
        >,
    ) {
        use broadcast::error::RecvError;

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
                        }
                        Ok(ServerReloadCommand::UpdateInPlace(ListenQuicInPlaceConfig::ListenConfig(config))) => {
                            self.update_socket_opts(&raw_socket, config);
                        }
                        Ok(ServerReloadCommand::UpdateInPlace(ListenQuicInPlaceConfig::QuinnConfig(config))) => {
                            listener.set_server_config(Some(config));
                        }
                        Ok(ServerReloadCommand::UpdateInPlace(ListenQuicInPlaceConfig::IngressAcl(ingress_net_filter))) => {
                            self.ingress_net_filter = ingress_net_filter;
                        }
                        Ok(ServerReloadCommand::UpdateInPlace(ListenQuicInPlaceConfig::AcceptTimeout(timeout))) => {
                            self.accept_timeout = timeout;
                        }
                        Ok(ServerReloadCommand::QuitRuntime) => break,
                        Err(RecvError::Closed) => break,
                        Err(RecvError::Lagged(dropped)) => {
                            warn!("SRT[{}_v{}#{}] server {} reload notify channel overflowed, {dropped} msg dropped",
                                self.server.name(), self.server_version, self.instance_id, self.server.name());
                        },
                    }
                }
                result = listener.accept() => {
                    let Some(incoming) = result else {
                        continue;
                    };
                    self.listen_stats.add_accepted();
                    self.run_task(incoming);
                }
            }
        }

        info!(
            "SRT[{}_v{}#{}] will go offline",
            self.server.name(),
            self.server_version,
            self.instance_id
        );
        self.pre_stop();
        self.goto_offline(listener).await;
        self.post_stop();
    }

    fn run_task(&self, incoming: Incoming) {
        let peer_addr = incoming.remote_address();
        if let Some(filter) = &self.ingress_net_filter {
            let (_, action) = filter.check(peer_addr.ip());
            match action {
                AclAction::Permit | AclAction::PermitAndLog => {}
                AclAction::Forbid | AclAction::ForbidAndLog => {
                    self.listen_stats.add_dropped();
                    incoming.ignore();
                    return;
                }
            }
        }

        let local_addr = incoming
            .local_ip()
            .map(|ip| SocketAddr::new(ip, self.listen_addr.port()))
            .unwrap_or(self.listen_addr);
        let mut cc_info =
            ClientConnectionInfo::new(peer_addr.to_canonical(), local_addr.to_canonical());

        let server = self.server.clone();
        let listen_stats = self.listen_stats.clone();
        let accept_timeout = self.accept_timeout;
        if let Some(worker_id) = self.worker_id {
            cc_info.set_worker_id(Some(worker_id));
            tokio::spawn(async move {
                Self::accept_connection_and_run(
                    server,
                    incoming,
                    cc_info,
                    accept_timeout,
                    listen_stats,
                )
                .await
            });
        } else if let Some(rt) = crate::runtime::worker::select_handle() {
            cc_info.set_worker_id(Some(rt.id));
            rt.handle.spawn(async move {
                Self::accept_connection_and_run(
                    server,
                    incoming,
                    cc_info,
                    accept_timeout,
                    listen_stats,
                )
                .await
            });
        } else {
            tokio::spawn(async move {
                Self::accept_connection_and_run(
                    server,
                    incoming,
                    cc_info,
                    accept_timeout,
                    listen_stats,
                )
                .await
            });
        }
    }

    async fn accept_connection_and_run(
        server: S,
        incoming: Incoming,
        cc_info: ClientConnectionInfo,
        timeout: Duration,
        listen_stats: Arc<ListenStats>,
    ) {
        let connecting = match incoming.accept() {
            Ok(c) => c,
            Err(_e) => {
                listen_stats.add_failed();
                // TODO may be attack
                return;
            }
        };
        match tokio::time::timeout(timeout, connecting).await {
            Ok(Ok(c)) => {
                listen_stats.add_accepted();
                server.run_quic_task(c, cc_info).await;
            }
            Ok(Err(_e)) => {
                listen_stats.add_failed();
                // TODO may be attack
            }
            Err(_) => {
                listen_stats.add_failed();
                // TODO may be attack
            }
        }
    }

    fn update_socket_opts(&mut self, raw_socket: &RawSocket, config: UdpListenConfig) {
        if self.listen_config.socket_misc_opts() != config.socket_misc_opts() {
            match raw_socket.set_udp_misc_opts(self.listen_addr, config.socket_misc_opts()) {
                Ok(_) => {
                    self.listen_config
                        .set_socket_misc_opts(config.socket_misc_opts());
                }
                Err(e) => {
                    warn!(
                        "SRT[{}_v{}#{}] update socket misc opts failed: {e}",
                        self.server.name(),
                        self.server_version,
                        self.instance_id,
                    );
                }
            }
        }

        if self.listen_config.socket_buffer() != config.socket_buffer() {
            match raw_socket.set_buf_opts(config.socket_buffer()) {
                Ok(_) => {
                    self.listen_config
                        .set_socket_misc_opts(config.socket_misc_opts());
                }
                Err(e) => {
                    warn!(
                        "SRT[{}_v{}#{}] update socket buf opts failed: {e}",
                        self.server.name(),
                        self.server_version,
                        self.instance_id,
                    );
                }
            }
        }
    }

    async fn goto_offline(&self, listener: Endpoint) {
        let mut timeout = Box::pin(tokio::time::sleep(self.accept_timeout));

        loop {
            tokio::select! {
                biased;

                _ = &mut timeout => {
                    break;
                }
                result = listener.accept() => {
                    let Some(incoming) = result else {
                        continue;
                    };
                    self.listen_stats.add_accepted();
                    self.run_task(incoming);
                }
            }
        }

        listener.wait_idle().await;
        self.goto_close(listener);
    }

    fn goto_close(&self, listener: Endpoint) {
        info!(
            "SRT[{}_v{}#{}] will close all quic connections immediately",
            self.server.name(),
            self.server_version,
            self.instance_id
        );
        listener.close(quinn::VarInt::default(), b"close as server shutdown");
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
        socket: UdpSocket,
        config: quinn::ServerConfig,
        server_reload_channel: broadcast::Receiver<ServerReloadCommand<ListenQuicInPlaceConfig>>,
    ) {
        let handle = self.get_rt_handle();
        handle.spawn(async move {
            let raw_socket = RawSocket::from(&socket);
            // make sure the listen socket associated with the correct reactor
            match Endpoint::new(
                Default::default(),
                Some(config),
                socket,
                Arc::new(quinn::TokioRuntime),
            ) {
                Ok(endpoint) => {
                    self.pre_start();
                    self.run(endpoint, raw_socket, server_reload_channel).await;
                }
                Err(e) => {
                    warn!(
                        "SRT[{}_v{}#{}] listen async: {e:?}",
                        self.server.name(),
                        self.server_version,
                        self.instance_id
                    );
                }
            }
        });
    }
}
