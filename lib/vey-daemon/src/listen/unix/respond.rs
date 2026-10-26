/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! One Unix datagram in, one datagram back.
//!
//! [`UnixDatagramResponder::send_to`] names the destination client address.
//! A direct reply uses the address returned with the request. A delayed reply
//! keeps a clone of the responder and that address until the response exists.

use std::future::Future;
use std::io;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::Arc;

use log::{debug, info, warn};
use tokio::io::Interest;
use tokio::net::UnixDatagram;
use tokio::net::unix::SocketAddr as UnixSocketAddr;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::server::{BaseServer, ReloadServer, ServerReloadCommand};

/// Sends one response datagram to an explicit Unix client address.
#[derive(Clone)]
pub struct UnixDatagramResponder {
    socket: Arc<UnixDatagram>,
}

impl UnixDatagramResponder {
    /// Send `packet` to `client_addr`.
    ///
    /// `client_addr` is the destination. Pathname addresses are the usual
    /// bound client sockets. Abstract-namespace addresses are accepted on
    /// Linux. An unnamed peer has nowhere to send.
    pub async fn send_to(&self, packet: &[u8], client_addr: &UnixSocketAddr) -> io::Result<()> {
        let n = if let Some(path) = client_addr.as_pathname() {
            self.socket.send_to(packet, path).await?
        } else {
            self.send_abstract(packet, client_addr).await?
        };
        if n != packet.len() {
            return Err(io::Error::other(format!(
                "not all data written, only {n}/{}",
                packet.len()
            )));
        }
        Ok(())
    }

    async fn send_abstract(
        &self,
        packet: &[u8],
        client_addr: &UnixSocketAddr,
    ) -> io::Result<usize> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        if let Some(name) = client_addr.as_abstract_name() {
            let fd = self.socket.as_raw_fd();
            return self
                .socket
                .async_io(Interest::WRITABLE, || send_abstract(fd, packet, name))
                .await;
        }

        let _ = (packet, client_addr);
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "unix datagram client address is unnamed",
        ))
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn send_abstract(fd: std::os::unix::io::RawFd, packet: &[u8], name: &[u8]) -> io::Result<usize> {
    let mut storage = unsafe { std::mem::zeroed::<libc::sockaddr_un>() };
    storage.sun_family = libc::AF_UNIX as libc::sa_family_t;
    if name.len() + 1 >= storage.sun_path.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "abstract unix name is too long",
        ));
    }
    storage.sun_path[0] = 0;
    for (i, byte) in name.iter().enumerate() {
        storage.sun_path[i + 1] = *byte as libc::c_char;
    }
    let len =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + 1 + name.len()) as libc::socklen_t;

    loop {
        let n = unsafe {
            libc::sendto(
                fd,
                packet.as_ptr().cast::<libc::c_void>(),
                packet.len(),
                0,
                (&raw const storage).cast::<libc::sockaddr>(),
                len,
            )
        };
        if n >= 0 {
            return Ok(n as usize);
        }
        let err = io::Error::last_os_error();
        if err.kind() == io::ErrorKind::Interrupted {
            continue;
        }
        return Err(err);
    }
}

/// Handles one request datagram.
///
/// The runtime does not send a response. Call [`UnixDatagramResponder::send_to`]
/// with the client address that should receive it.
pub trait RespondUnixDatagramServer: BaseServer {
    fn handle_unix_request<'a>(
        &'a self,
        packet: &'a [u8],
        client_addr: &'a UnixSocketAddr,
        responder: &'a UnixDatagramResponder,
    ) -> impl Future<Output = ()> + Send + 'a;
}

/// Receives one Unix datagram request at a time and hands the caller a
/// [`UnixDatagramResponder`].
#[derive(Clone)]
pub struct RespondUnixDatagramRuntime<S> {
    server: S,
    server_type: &'static str,
    server_version: usize,
    listen_path: PathBuf,
    listen_path_create_time: i64,
}

impl<S> RespondUnixDatagramRuntime<S>
where
    S: RespondUnixDatagramServer + ReloadServer + Clone + Send + Sync + 'static,
{
    pub fn new(server: S, listen_path: PathBuf) -> Self {
        let server_type = server.r#type();
        let server_version = server.version();
        RespondUnixDatagramRuntime {
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
    }

    async fn run(
        mut self,
        socket: UnixDatagram,
        mut server_reload_channel: broadcast::Receiver<ServerReloadCommand<()>>,
    ) {
        use broadcast::error::RecvError;

        let responder = UnixDatagramResponder {
            socket: Arc::new(socket),
        };
        let mut buf = [0u8; u16::MAX as usize];
        loop {
            tokio::select! {
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
                        Ok(ServerReloadCommand::UpdateInPlace(_c)) => {
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
                r = responder.socket.recv_from(&mut buf) => {
                    match r {
                        Ok((len, client_addr)) => {
                            self.server
                                .handle_unix_request(&buf[..len], &client_addr, &responder)
                                .await;
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
        server_reload_sender: &broadcast::Sender<ServerReloadCommand<()>>,
    ) -> anyhow::Result<JoinHandle<()>> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};

        match self.listen_path.symlink_metadata() {
            Ok(metadata) => {
                if !metadata.file_type().is_socket() {
                    return Err(anyhow::anyhow!(
                        "listen socket path {} exists but is not a socket",
                        self.listen_path.display()
                    ));
                }
                if metadata.uid() != rustix::process::getuid().as_raw() {
                    return Err(anyhow::anyhow!(
                        "listen socket path {} belongs to a different uid {}",
                        self.listen_path.display(),
                        metadata.uid()
                    ));
                }
                std::fs::remove_file(&self.listen_path).map_err(|e| {
                    anyhow::anyhow!("failed to remove old {}: {e}", self.listen_path.display())
                })?;
            }
            Err(e) => {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(anyhow::anyhow!(
                        "failed to check existed socket file {}: {e}",
                        self.listen_path.display()
                    ));
                }
            }
        }

        let socket = UnixDatagram::bind(&self.listen_path).map_err(|e| {
            anyhow::anyhow!(
                "failed to create unix datagram socket on path {}: {e}",
                self.listen_path.display()
            )
        })?;

        let current_uid = rustix::process::getuid().as_raw();
        let metadata = self.listen_path.metadata().map_err(|e| {
            anyhow::anyhow!(
                "failed to get metadata on control socket path {}: {e}",
                self.listen_path.display()
            )
        })?;
        if !metadata.file_type().is_socket() || metadata.uid() != current_uid {
            return Err(anyhow::anyhow!(
                "control socket path {} has been deleted",
                self.listen_path.display()
            ));
        }
        self.listen_path_create_time = metadata.ctime();

        let server_reload_channel = server_reload_sender.subscribe();
        let handle = tokio::spawn(async move {
            self.pre_start();
            self.run(socket, server_reload_channel).await;
        });
        Ok(handle)
    }
}

impl<S> Drop for RespondUnixDatagramRuntime<S> {
    fn drop(&mut self) {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};

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

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::str::FromStr;
    use std::sync::Mutex;
    use std::time::Duration;

    use tokio::time::timeout;
    use vey_types::metrics::NodeName;

    #[derive(Clone)]
    struct TestServer {
        name: NodeName,
        reply_to: Arc<Mutex<Option<UnixSocketAddr>>>,
    }

    impl BaseServer for TestServer {
        fn name(&self) -> &NodeName {
            &self.name
        }

        fn r#type(&self) -> &'static str {
            "test-respond-unix"
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

    impl RespondUnixDatagramServer for TestServer {
        fn handle_unix_request<'a>(
            &'a self,
            packet: &'a [u8],
            client_addr: &'a UnixSocketAddr,
            responder: &'a UnixDatagramResponder,
        ) -> impl Future<Output = ()> + Send + 'a {
            let dest = self
                .reply_to
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| client_addr.clone());
            async move {
                let _ = packet;
                responder.send_to(b"pong", &dest).await.unwrap();
            }
        }
    }

    struct DirGuard(PathBuf);

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn socket_addr(path: &Path) -> UnixSocketAddr {
        let addr = std::os::unix::net::SocketAddr::from_pathname(path).unwrap();
        UnixSocketAddr::from(addr)
    }

    #[tokio::test]
    async fn response_goes_to_the_named_client() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vey-respond-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _guard = DirGuard(dir.clone());

        let server_path = dir.join("server");
        let sender_path = dir.join("sender");
        let other_path = dir.join("other");
        let sender = UnixDatagram::bind(&sender_path).unwrap();
        let other = UnixDatagram::bind(&other_path).unwrap();

        let reply_to = Arc::new(Mutex::new(Some(socket_addr(&other_path))));
        let server = TestServer {
            name: NodeName::from_str("respond-unix").unwrap(),
            reply_to,
        };
        let runtime = RespondUnixDatagramRuntime::new(server, server_path.clone());
        let reload = ServerReloadCommand::new_sender();
        let handle = runtime.spawn(&reload).unwrap();

        sender.send_to(b"ping", &server_path).await.unwrap();

        let mut buf = [0u8; 16];
        let (n, from) = timeout(Duration::from_secs(2), other.recv_from(&mut buf))
            .await
            .expect("timed out waiting for datagram")
            .unwrap();
        assert_eq!(&buf[..n], b"pong");
        assert_eq!(from.as_pathname(), Some(server_path.as_path()));

        let missed = timeout(Duration::from_millis(200), sender.recv_from(&mut buf)).await;
        assert!(
            missed.is_err(),
            "sender should not receive a reply aimed at other"
        );

        let _ = reload.send(ServerReloadCommand::QuitRuntime);
        timeout(Duration::from_secs(2), handle)
            .await
            .expect("runtime did not stop")
            .unwrap();
    }
}
