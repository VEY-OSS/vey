/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::anyhow;
use redis::aio::MultiplexedConnection;
use redis::{AsyncCommands, AsyncConnectionConfig, ProtocolVersion, RedisConnectionInfo};
use rustls_pki_types::ServerName;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use vey_types::net::{Host, RustlsClientConfig, RustlsClientConfigBuilder, UpstreamAddr};

#[cfg(feature = "yaml")]
mod yaml;

pub const REDIS_DEFAULT_PORT: u16 = 6379;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RedisClientConfigBuilder {
    addr: UpstreamAddr,
    tls_client: Option<RustlsClientConfigBuilder>,
    tls_name: Option<ServerName<'static>>,
    db: i64,
    username: Option<String>,
    password: Option<String>,
    connect_timeout: Duration,
    response_timeout: Duration,
}

struct RedisClientTlsConfig {
    tls_config: RustlsClientConfig,
    tls_name: ServerName<'static>,
}

pub struct RedisClientConfig {
    server: UpstreamAddr,
    tls_client: Option<RedisClientTlsConfig>,
    db_info: RedisConnectionInfo,
    connect_timeout: Duration,
    response_timeout: Duration,
}

impl Default for RedisClientConfigBuilder {
    fn default() -> Self {
        RedisClientConfigBuilder::new(UpstreamAddr::new(
            Host::Ip(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            REDIS_DEFAULT_PORT,
        ))
    }
}

impl RedisClientConfigBuilder {
    pub fn new(server: UpstreamAddr) -> Self {
        RedisClientConfigBuilder {
            addr: server,
            tls_client: None,
            tls_name: None,
            db: 0,
            username: None,
            password: None,
            connect_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(2),
        }
    }

    pub fn set_addr(&mut self, addr: UpstreamAddr) {
        self.addr = addr;
    }

    pub fn set_tls_client(&mut self, tls: RustlsClientConfigBuilder) {
        self.tls_client = Some(tls);
    }

    pub fn set_tls_name(&mut self, name: ServerName<'static>) {
        self.tls_name = Some(name);
    }

    pub fn set_db(&mut self, db: i64) {
        self.db = db;
    }

    pub fn set_username(&mut self, name: String) {
        self.username = Some(name);
    }

    pub fn set_password(&mut self, password: String) {
        self.password = Some(password);
    }

    pub fn set_connect_timeout(&mut self, timeout: Duration) {
        self.connect_timeout = timeout;
    }

    pub fn set_response_timeout(&mut self, timeout: Duration) {
        self.response_timeout = timeout;
    }

    pub fn build(&self) -> anyhow::Result<RedisClientConfig> {
        let mut connection_info = RedisConnectionInfo::default()
            .set_db(self.db)
            .set_protocol(ProtocolVersion::RESP3);
        if let Some(name) = &self.username {
            connection_info = connection_info.set_username(name);
        }
        if let Some(password) = &self.password {
            connection_info = connection_info.set_password(password);
        }
        let mut client = RedisClientConfig {
            server: self.addr.clone(),
            tls_client: None,
            db_info: connection_info,
            connect_timeout: self.connect_timeout,
            response_timeout: self.response_timeout,
        };

        if let Some(config) = &self.tls_client {
            let tls_config = config.build()?;
            let tls_name = if let Some(name) = &self.tls_name {
                name.clone()
            } else {
                ServerName::try_from(self.addr.host())
                    .map_err(|e| anyhow!("invalid tls server name: {e}"))?
            };
            client.tls_client = Some(RedisClientTlsConfig {
                tls_config,
                tls_name,
            });
        }

        Ok(client)
    }
}

impl RedisClientConfig {
    async fn lookup_server(&self) -> anyhow::Result<Vec<SocketAddr>> {
        match self.server.host() {
            Host::Domain(domain) => {
                let ips = match tokio::time::timeout(
                    self.connect_timeout,
                    tokio::net::lookup_host((domain.as_str(), self.server.port())),
                )
                .await
                {
                    Ok(Ok(ips)) => ips,
                    Ok(Err(e)) => {
                        return Err(anyhow!("failed to resolve domain {domain}: {e}"));
                    }
                    Err(_) => return Err(anyhow!("timeout to resolve domain {domain}")),
                };
                let addrs: Vec<SocketAddr> = ips.collect();
                if addrs.is_empty() {
                    return Err(anyhow!("no ip address resolved for domain {domain}"));
                }
                Ok(addrs)
            }
            Host::Ip(ip) => Ok(vec![SocketAddr::new(*ip, self.server.port())]),
        }
    }

    async fn tcp_connect(&self, peer: SocketAddr) -> anyhow::Result<TcpStream> {
        let socket = vey_socket::tcp::new_socket_to(
            peer.ip(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            true,
        )
        .map_err(|e| anyhow!("failed to create new socket: {e}"))?;

        match tokio::time::timeout(self.connect_timeout, socket.connect(peer)).await {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(e)) => Err(anyhow!("failed to connect to {}: {e}", self.server)),
            Err(_) => Err(anyhow!("timeout to connect to {}", self.server)),
        }
    }

    pub async fn connect(&self) -> anyhow::Result<impl AsyncCommands + use<>> {
        let peers = self.lookup_server().await?;
        self.connect_peers(peers).await
    }

    async fn connect_peers(&self, peers: Vec<SocketAddr>) -> anyhow::Result<MultiplexedConnection> {
        let mut last_err = None;
        for peer in peers {
            let stream = match self.tcp_connect(peer).await {
                Ok(stream) => stream,
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };

            return if let Some(tls_client) = &self.tls_client {
                let tls_connector = TlsConnector::from(tls_client.tls_config.driver.clone());
                match tokio::time::timeout(
                    tls_client.tls_config.handshake_timeout,
                    tls_connector.connect(tls_client.tls_name.clone(), stream),
                )
                .await
                {
                    Ok(Ok(stream)) => self.redis_handshake(stream).await,
                    Ok(Err(e)) => Err(anyhow!("failed to tls handshake with {}: {e}", self.server)),
                    Err(_) => Err(anyhow!("timeout to tls handshake with {}", self.server)),
                }
            } else {
                self.redis_handshake(stream).await
            };
        }

        Err(last_err.unwrap_or_else(|| anyhow!("no ip address resolved for {}", self.server)))
    }

    async fn redis_handshake<S>(&self, stream: S) -> anyhow::Result<MultiplexedConnection>
    where
        S: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        let async_config =
            AsyncConnectionConfig::new().set_response_timeout(Some(self.response_timeout));

        let handshake = MultiplexedConnection::new_with_config(&self.db_info, stream, async_config);
        let (conn, background) = match tokio::time::timeout(self.response_timeout, handshake).await
        {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                return Err(anyhow!("redis handshake with {} failed: {e}", self.server));
            }
            Err(_) => return Err(anyhow!("timeout to redis handshake with {}", self.server)),
        };
        tokio::spawn(background);
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::str::FromStr;
    use std::sync::Once;
    use std::time::Duration;

    use rustls_pki_types::ServerName;
    use vey_types::net::{Host, RustlsClientConfigBuilder, UpstreamAddr};

    use super::*;

    fn init_test_rustls_provider() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            rustls::crypto::ring::default_provider()
                .install_default()
                .expect("test rustls ring provider should install once");
        });
    }

    #[test]
    fn default_builder_uses_localhost_and_redis_port() {
        let builder = RedisClientConfigBuilder::default();
        assert_eq!(
            builder.addr,
            UpstreamAddr::new(
                Host::Ip(IpAddr::from_str("127.0.0.1").unwrap()),
                REDIS_DEFAULT_PORT,
            )
        );
        assert_eq!(builder.connect_timeout, Duration::from_secs(5));
        assert_eq!(builder.response_timeout, Duration::from_secs(2));
        assert_eq!(builder.db, 0);
        assert!(builder.tls_client.is_none());
    }

    #[test]
    fn build_plain_config_sets_resp3_and_credentials() {
        let mut builder = RedisClientConfigBuilder::default();
        builder.set_db(3);
        builder.set_username("user".to_string());
        builder.set_password("secret".to_string());
        builder.set_connect_timeout(Duration::from_secs(11));
        builder.set_response_timeout(Duration::from_secs(4));

        let config = builder.build().unwrap();
        assert_eq!(config.server.port(), REDIS_DEFAULT_PORT);
        assert_eq!(config.connect_timeout, Duration::from_secs(11));
        assert_eq!(config.response_timeout, Duration::from_secs(4));
        assert!(config.tls_client.is_none());
    }

    #[test]
    fn build_tls_config_uses_explicit_server_name() {
        init_test_rustls_provider();
        let mut builder = RedisClientConfigBuilder::default();
        builder.set_tls_client(RustlsClientConfigBuilder::default());
        builder.set_tls_name(ServerName::try_from("redis.internal").unwrap());

        let config = builder.build().unwrap();
        let tls = config.tls_client.as_ref().unwrap();
        assert_eq!(
            tls.tls_name,
            ServerName::try_from("redis.internal").unwrap()
        );
    }

    #[test]
    fn build_tls_config_derives_server_name_from_ip_host() {
        init_test_rustls_provider();
        let mut builder = RedisClientConfigBuilder::new(UpstreamAddr::new(
            Host::Ip(IpAddr::from_str("10.0.0.5").unwrap()),
            6379,
        ));
        builder.set_tls_client(RustlsClientConfigBuilder::default());

        let config = builder.build().unwrap();
        let tls = config.tls_client.as_ref().unwrap();
        assert_eq!(tls.tls_name, ServerName::try_from("10.0.0.5").unwrap());
    }

    fn silent_peer_config(port: u16) -> RedisClientConfig {
        let mut builder = RedisClientConfigBuilder::new(UpstreamAddr::new(
            Host::Ip(IpAddr::from_str("127.0.0.1").unwrap()),
            port,
        ));
        builder.set_connect_timeout(Duration::from_millis(500));
        builder.set_response_timeout(Duration::from_millis(200));
        builder.build().unwrap()
    }

    #[tokio::test]
    async fn handshake_times_out_when_peer_is_silent() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _hold = sock;
            std::future::pending::<()>().await;
        });

        let config = silent_peer_config(addr.port());
        let started = std::time::Instant::now();
        let err = match config.connect().await {
            Ok(_) => panic!("silent peer should not finish handshake"),
            Err(e) => e,
        };
        let elapsed = started.elapsed();

        assert!(
            err.to_string().contains("timeout to redis handshake"),
            "{err}"
        );
        assert!(elapsed >= Duration::from_millis(150), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
    }

    #[tokio::test]
    async fn connect_tries_next_address_after_refusal() {
        let closed = {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            drop(listener);
            addr
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            let _hold = sock;
            std::future::pending::<()>().await;
        });

        let config = silent_peer_config(open.port());
        let err = match config.connect_peers(vec![closed, open]).await {
            Ok(_) => panic!("closed then silent peer should not finish handshake"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("timeout to redis handshake"),
            "{err}"
        );
    }
}
