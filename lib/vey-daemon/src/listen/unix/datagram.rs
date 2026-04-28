/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::PathBuf;

use anyhow::anyhow;
use log::{debug, info, warn};
use tokio::net::UnixDatagram;
use tokio::net::unix::SocketAddr as UnixSocketAddr;
use tokio::sync::broadcast;

use crate::server::{BaseServer, ReloadServer, ServerReloadCommand};

pub trait ReceiveUnixDatagramServer: BaseServer {
    fn receive_unix_packet(&self, packet: &[u8], peer_addr: UnixSocketAddr);
}

#[derive(Clone)]
pub struct ReceiveUnixDatagramRuntime<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    listen_path: PathBuf,
    listen_path_create_time: i64,
    //listen_stats: Arc<ListenStats>,
}

impl<S> ReceiveUnixDatagramRuntime<S>
where
    S: ReceiveUnixDatagramServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(server: S, listen_path: PathBuf) -> Self {
        let server_type = server.r#type();
        let server_version = server.version();
        ReceiveUnixDatagramRuntime {
            server,
            server_type,
            server_version,
            listen_path,
            listen_path_create_time: 0,
        }
    }

    fn pre_start(&self) {
        info!(
            "started {} SRT[{}_v{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
        );
        //self.listen_stats.add_running_runtime();
    }

    fn pre_stop(&self) {
        info!(
            "stopping {} SRT[{}_v{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
        );
    }

    fn post_stop(&self) {
        info!(
            "stopped {} SRT[{}_v{}]",
            self.server_type,
            self.server.name(),
            self.server_version,
        );
        //self.listen_stats.del_running_runtime();
    }

    async fn run(
        mut self,
        socket: UnixDatagram,
        mut server_reload_channel: broadcast::Receiver<ServerReloadCommand>,
    ) {
        use broadcast::error::RecvError;

        let mut buf = [0u8; u16::MAX as usize];
        loop {
            tokio::select! {
                biased;

                ev = server_reload_channel.recv() => {
                    match ev {
                        Ok(ServerReloadCommand::ReloadVersion(version)) => {
                            info!("SRT[{}_v{}] received reload request from v{version}",
                                self.server.name(), self.server_version);
                            let new_server = self.server.reload();
                            self.server_version = new_server.version();
                            self.server = new_server;
                            continue;
                        }
                        Ok(ServerReloadCommand::QuitRuntime) => {},
                        Err(RecvError::Closed) => {},
                        Err(RecvError::Lagged(dropped)) => {
                            warn!("SRT[{}_v{}] server {} reload notify channel overflowed, {dropped} msg dropped",
                                self.server.name(), self.server_version, self.server.name());
                            continue;
                        }
                    }

                    info!("SRT[{}_v{}] will go offline",
                        self.server.name(), self.server_version);
                    self.pre_stop();
                    break;
                }
                r = socket.recv_from(&mut buf) => {
                    match r {
                        Ok((len, peer_addr)) => {
                            // TODO add stats
                            self.server.receive_unix_packet(&buf[..len], peer_addr);
                        }
                        Err(e) => {
                            warn!("SRT[{}_v{}] error receiving data from socket, error: {e}",
                                self.server.name(), self.server_version);
                        }
                    }
                }
            }
        }

        self.post_stop();
    }

    pub fn spawn(
        mut self,
        server_reload_sender: &broadcast::Sender<ServerReloadCommand>,
    ) -> anyhow::Result<()> {
        match self.listen_path.symlink_metadata() {
            Ok(metadata) => {
                if !metadata.file_type().is_socket() {
                    return Err(anyhow!(
                        "listen socket path {} exists but is not a socket",
                        self.listen_path.display()
                    ));
                }
                if metadata.uid() != rustix::process::getuid().as_raw() {
                    return Err(anyhow!(
                        "listen socket path {} belongs to a different uid {}",
                        self.listen_path.display(),
                        metadata.uid()
                    ));
                }
                std::fs::remove_file(&self.listen_path).map_err(|e| {
                    anyhow!("failed to remove old {}: {e}", self.listen_path.display())
                })?;
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(anyhow!(
                        "failed to check existed socket file {}: {e}",
                        self.listen_path.display()
                    ));
                }
            }
        }

        let socket = UnixDatagram::bind(&self.listen_path).map_err(|e| {
            anyhow!(
                "failed to create unix datagram socket on path {}: {e}",
                self.listen_path.display()
            )
        })?;

        let current_uid = rustix::process::getuid().as_raw();
        let metadata = self.listen_path.metadata().map_err(|e| {
            anyhow!(
                "failed to get metadata on control socket path {}: {e}",
                self.listen_path.display()
            )
        })?;
        if !metadata.file_type().is_socket() || metadata.uid() != current_uid {
            return Err(anyhow!(
                "control socket path {} has been deleted",
                self.listen_path.display()
            ));
        }
        self.listen_path_create_time = metadata.ctime();

        let server_reload_channel = server_reload_sender.subscribe();
        tokio::spawn(async move {
            self.pre_start();
            self.run(socket, server_reload_channel).await;
        });
        Ok(())
    }
}

impl<S> Drop for ReceiveUnixDatagramRuntime<S> {
    fn drop(&mut self) {
        if let Ok(metadata) = self.listen_path.symlink_metadata() {
            if !metadata.file_type().is_socket() {
                return;
            }
            if metadata.ctime() != self.listen_path_create_time {
                return;
            }
            debug!("unlink socket file {}", self.listen_path.display());
            if let Err(e) = std::fs::remove_file(&self.listen_path) {
                warn!(
                    "failed to unlink datagram listen socket {}: {e}",
                    self.listen_path.display()
                );
            }
        }
    }
}
