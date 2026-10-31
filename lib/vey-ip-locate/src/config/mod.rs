/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
#[cfg(unix)]
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;

use vey_types::net::SocketBufferConfig;

use super::query::StdQuerySocket;
use super::{IpLocationQueryRuntime, IpLocationServiceHandle};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QueryPeer {
    Udp(SocketAddr),
    #[cfg(unix)]
    Unix(PathBuf),
}

#[cfg(feature = "yaml")]
mod yaml;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IpLocateServiceConfig {
    pub(crate) cache_request_batch_count: NonZeroUsize,
    pub(crate) cache_request_timeout: Duration,
    pub(crate) query_peer: QueryPeer,
    pub(crate) query_socket_buffer: SocketBufferConfig,
    pub(crate) query_wait_timeout: Duration,
    pub(crate) default_expire_ttl: u32,
    pub(crate) maximum_expire_ttl: u32,
}

impl Default for IpLocateServiceConfig {
    fn default() -> Self {
        IpLocateServiceConfig {
            cache_request_batch_count: NonZeroUsize::new(10).unwrap(),
            cache_request_timeout: Duration::from_secs(2),
            query_peer: QueryPeer::Udp(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 2888)),
            query_socket_buffer: SocketBufferConfig::default(),
            query_wait_timeout: Duration::from_secs(1),
            default_expire_ttl: 10,
            maximum_expire_ttl: 300,
        }
    }
}

impl IpLocateServiceConfig {
    pub fn set_cache_request_batch_count(&mut self, count: NonZeroUsize) {
        self.cache_request_batch_count = count;
    }

    pub fn set_cache_request_timeout(&mut self, time: Duration) {
        self.cache_request_timeout = time;
    }

    pub(crate) fn set_query_peer(&mut self, peer: QueryPeer) {
        self.query_peer = peer;
    }

    pub fn set_query_peer_addr(&mut self, addr: SocketAddr) {
        self.set_query_peer(QueryPeer::Udp(addr));
    }

    #[cfg(unix)]
    pub fn set_query_unix_peer(&mut self, path: PathBuf) {
        self.set_query_peer(QueryPeer::Unix(path));
    }

    pub fn set_query_socket_buffer(&mut self, config: SocketBufferConfig) {
        self.query_socket_buffer = config;
    }

    pub fn set_query_wait_timeout(&mut self, time: Duration) {
        self.query_wait_timeout = time;
    }

    pub fn set_default_expire_ttl(&mut self, ttl: u32) {
        self.default_expire_ttl = ttl;
    }

    pub fn set_maximum_expire_ttl(&mut self, ttl: u32) {
        self.maximum_expire_ttl = ttl;
    }

    fn open_std_query_socket(&self) -> anyhow::Result<StdQuerySocket> {
        use anyhow::Context;

        match &self.query_peer {
            QueryPeer::Udp(addr) => {
                let socket = vey_socket::udp::new_std_socket_to(
                    *addr,
                    &Default::default(),
                    self.query_socket_buffer,
                    Default::default(),
                )
                .context("failed to setup udp socket")?;
                socket
                    .connect(*addr)
                    .map_err(|e| anyhow!("failed to connect to peer address {addr}: {e:?}"))?;
                Ok(StdQuerySocket::Udp(socket))
            }
            #[cfg(unix)]
            QueryPeer::Unix(path) => {
                let socket = vey_socket::unix::new_std_connected_datagram(
                    path,
                    "vey-ip-locate",
                    self.query_socket_buffer.send_size(),
                )
                .with_context(|| {
                    format!("failed to setup unix datagram socket {}", path.display())
                })?;
                Ok(StdQuerySocket::Unix(socket))
            }
        }
    }

    fn spawn_query_runtime(
        &self,
        socket: StdQuerySocket,
        query_handle: super::IpLocationQueryHandle,
    ) -> anyhow::Result<()> {
        use anyhow::Context;

        if let Some(rt) = crate::get_ip_locate_rt_handle() {
            let config = self.clone();
            rt.spawn(async move {
                let socket = socket.into_tokio().expect("failed to setup query socket");
                IpLocationQueryRuntime::new(&config, socket, query_handle).await
            });
        } else {
            let socket = socket
                .into_tokio()
                .context("failed to setup query socket")?;
            tokio::spawn(IpLocationQueryRuntime::new(self, socket, query_handle));
        }
        Ok(())
    }

    pub fn spawn_ip_locate_agent(&self) -> anyhow::Result<IpLocationServiceHandle> {
        let socket = self.open_std_query_socket()?;

        let (cache_runtime, cache_handle, query_handle) = super::crate_ip_location_cache(self);
        self.spawn_query_runtime(socket, query_handle)?;
        if let Some(rt) = crate::get_ip_locate_rt_handle() {
            rt.spawn(cache_runtime);
        } else {
            tokio::spawn(cache_runtime);
        }

        Ok(IpLocationServiceHandle::new(
            cache_handle,
            self.cache_request_timeout,
        ))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::net::UnixDatagram;

    use super::{IpLocateServiceConfig, StdQuerySocket};

    #[test]
    fn unix_peer_connects_and_exchanges() {
        let path = std::env::temp_dir().join(format!(
            "vey-ip-locate-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let server = UnixDatagram::bind(&path).unwrap();

        let mut config = IpLocateServiceConfig::default();
        config.set_query_unix_peer(path.clone());
        let StdQuerySocket::Unix(client) = config.open_std_query_socket().unwrap() else {
            panic!("expected a unix datagram socket");
        };

        client.send(b"ping").unwrap();
        let mut buf = [0u8; 8];
        let (n, addr) = server.recv_from(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"ping");
        let client_path = addr
            .as_pathname()
            .expect("client socket has a pathname")
            .to_path_buf();
        server.send_to(b"pong", &client_path).unwrap();
        let n = client.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"pong");

        drop(client);
        drop(server);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unix_peer_missing_path_errors() {
        let path = std::env::temp_dir().join(format!(
            "vey-ip-locate-missing-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let mut config = IpLocateServiceConfig::default();
        config.set_query_unix_peer(path);
        assert!(config.open_std_query_socket().is_err());
    }
}
