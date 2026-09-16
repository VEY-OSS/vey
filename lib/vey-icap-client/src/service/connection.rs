/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::Poll;

use anyhow::Context;
use futures_util::poll;
use tokio::io::{AsyncRead, AsyncWrite, BufReader};
use tokio_rustls::TlsConnector;

use vey_io_ext::{AsyncStream, LimitedBufReadExt};
use vey_types::net::{Host, RustlsClientConfig};

use super::IcapServiceConfig;

pub type IcapClientWriter = Box<dyn AsyncWrite + Send + Sync + Unpin>;
pub type IcapClientReader = BufReader<Box<dyn AsyncRead + Send + Sync + Unpin>>;

pub struct IcapClientConnection {
    pub reader: IcapClientReader,
    pub writer: IcapClientWriter,
    reader_clean: bool,
    writer_clean: bool,
    reused_connection: bool,
}

impl IcapClientConnection {
    pub(super) fn new<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Send + Sync + Unpin + 'static,
        W: AsyncWrite + Send + Sync + Unpin + 'static,
    {
        IcapClientConnection {
            reader: BufReader::new(Box::new(reader)),
            writer: Box::new(writer),
            reader_clean: true,
            writer_clean: true,
            reused_connection: false,
        }
    }

    pub fn is_reused(&self) -> bool {
        self.reused_connection
    }

    pub(super) fn mark_reused(&mut self) {
        self.reused_connection = true
    }

    pub(super) fn reusable(&self) -> bool {
        self.reader_clean && self.writer_clean
    }

    pub fn mark_reader_finished(&mut self) {
        self.reader_clean = true;
    }

    pub fn mark_writer_finished(&mut self) {
        self.writer_clean = true;
    }

    pub(super) fn mark_io_inuse(&mut self) {
        self.reader_clean = false;
        self.writer_clean = false;
    }

    pub(super) async fn probe_idle(&mut self) -> bool {
        matches!(poll!(self.reader.fill_wait_data()), Poll::Pending)
    }
}

pub(super) struct IcapConnector {
    config: Arc<IcapServiceConfig>,
    tls_client: Option<RustlsClientConfig>,
}

impl IcapConnector {
    pub(super) fn new(config: Arc<IcapServiceConfig>) -> anyhow::Result<Self> {
        let tls_client = match &config.tls_client {
            Some(builder) => {
                let client = builder
                    .build()
                    .context("failed to build TLS client config")?;
                Some(client)
            }
            None => None,
        };
        Ok(IcapConnector { config, tls_client })
    }

    async fn select_peer_addr(&self) -> io::Result<SocketAddr> {
        let upstream = &self.config.upstream;
        match upstream.host() {
            Host::Domain(domain) => {
                let mut addrs = tokio::net::lookup_host((domain.as_str(), upstream.port())).await?;
                addrs
                    .next()
                    .ok_or_else(|| io::Error::other("no resolved socket address"))
            }
            Host::Ip(ip) => Ok(SocketAddr::new(*ip, upstream.port())),
        }
    }

    pub(super) async fn create(&self) -> io::Result<IcapClientConnection> {
        #[cfg(unix)]
        if let Some(path) = &self.config.use_unix_socket
            && let Ok(socket) = tokio::net::UnixStream::connect(path).await
        {
            let (r, w) = socket.into_split();
            return Ok(IcapClientConnection::new(r, w));
        }

        let peer = self.select_peer_addr().await?;
        let socket = vey_socket::tcp::new_socket_to(
            peer.ip(),
            &Default::default(),
            &self.config.tcp_keepalive,
            &Default::default(),
            true,
        )?;
        let stream = tokio::time::timeout(self.config.tcp_connect_timeout, socket.connect(peer))
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "ICAP tcp connect timed out"))??;

        if let Some(client) = &self.tls_client {
            let tls_connector = TlsConnector::from(client.driver.clone());
            match tokio::time::timeout(
                client.handshake_timeout,
                tls_connector.connect(self.config.tls_name.clone(), stream),
            )
            .await
            {
                Ok(Ok(tls_stream)) => {
                    let (r, w) = tls_stream.into_split();
                    Ok(IcapClientConnection::new(r, w))
                }
                Ok(Err(e)) => Err(e),
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "tls handshake with ICAP server timed out",
                )),
            }
        } else {
            let (r, w) = stream.into_split();
            Ok(IcapClientConnection::new(r, w))
        }
    }
}

#[allow(unused_imports)]
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn probe_detects_closed_idle_connection() {
        let (client, server) = tokio::io::duplex(1024);

        let (reader, writer) = tokio::io::split(client);

        let mut conn = IcapClientConnection {
            reader: BufReader::new(Box::new(reader)),
            writer: Box::new(writer),
            reader_clean: true,
            writer_clean: true,
            reused_connection: false,
        };

        // Idle live connection: nothing readable.
        assert!(conn.probe_idle().await);

        // Simulate ICAP server closing its side.
        drop(server);

        tokio::task::yield_now().await;

        // EOF should now be observable.
        assert!(!conn.probe_idle().await);
    }

    #[tokio::test]
    async fn probe_rejects_unexpected_data() {
        let (client, mut server) = tokio::io::duplex(1024);
        let (reader, writer) = tokio::io::split(client);

        let mut conn = IcapClientConnection {
            reader: BufReader::new(Box::new(reader)),
            writer: Box::new(writer),
            reader_clean: true,
            writer_clean: true,
            reused_connection: false,
        };

        server.write_all(b"garbage").await.unwrap();

        tokio::task::yield_now().await;

        assert!(!conn.probe_idle().await);
    }
}
