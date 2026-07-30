/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::{Context, anyhow};
use constant_time_eq::constant_time_eq_64;
use openssl::md::Md;
use openssl::md_ctx::MdCtx;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use vey_openssl::SslConnector;
use vey_socket::BindAddr;
use vey_types::net::{Host, OpensslClientConfig, OpensslClientConfigBuilder, TcpKeepAliveConfig};

use super::FluentdConnection;

#[cfg(feature = "yaml")]
mod yaml;

const FLUENTD_DEFAULT_PORT: u16 = 24224;
const FLUENTD_HASH_SIZE: usize = 64;

#[derive(Clone)]
pub struct FluentdClientConfig {
    server_addr: SocketAddr,
    bind: BindAddr,
    shared_key: String,
    username: String,
    password: String,
    tcp_keepalive: TcpKeepAliveConfig,
    tls_client: Option<OpensslClientConfig>,
    tls_name: Option<Host>,
    hostname: String,
    pub(super) connect_timeout: Duration,
    pub(super) connect_delay: Duration,
    pub(super) write_timeout: Duration,
    pub(super) flush_interval: Duration,
    pub(super) retry_queue_len: usize,
}

impl Default for FluentdClientConfig {
    fn default() -> Self {
        FluentdClientConfig::new(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            FLUENTD_DEFAULT_PORT,
        ))
    }
}

impl FluentdClientConfig {
    pub fn new(server: SocketAddr) -> Self {
        let hostname = vey_compat::hostname().to_string_lossy().into_owned();
        FluentdClientConfig {
            server_addr: server,
            bind: BindAddr::None,
            shared_key: String::new(),
            username: String::new(),
            password: String::new(),
            tcp_keepalive: TcpKeepAliveConfig::default_enabled(),
            tls_client: None,
            tls_name: None,
            hostname,
            connect_timeout: Duration::from_secs(10),
            connect_delay: Duration::from_secs(10),
            write_timeout: Duration::from_secs(1),
            flush_interval: Duration::from_millis(100),
            retry_queue_len: 10,
        }
    }

    pub fn set_server_addr(&mut self, addr: SocketAddr) {
        self.server_addr = addr;
    }

    pub fn set_bind_ip(&mut self, ip: IpAddr) {
        self.bind = BindAddr::Ip(ip);
    }

    pub fn set_shared_key(&mut self, key: String) {
        self.shared_key = key;
    }

    pub fn set_username(&mut self, username: String) {
        self.username = username;
    }

    pub fn set_password(&mut self, password: String) {
        self.password = password;
    }

    pub fn set_hostname(&mut self, hostname: String) {
        self.hostname = hostname;
    }

    pub fn set_tcp_keepalive(&mut self, keepalive: TcpKeepAliveConfig) {
        self.tcp_keepalive = keepalive;
    }

    pub fn set_tls_client(&mut self, tls_config: OpensslClientConfigBuilder) -> anyhow::Result<()> {
        let tls_client = tls_config
            .build()
            .context("failed to build tls client config")?;
        self.tls_client = Some(tls_client);
        Ok(())
    }

    pub fn set_tls_name(&mut self, tls_name: Host) {
        self.tls_name = Some(tls_name);
    }

    pub fn set_connect_timeout(&mut self, timeout: Duration) {
        self.connect_timeout = timeout;
    }

    pub fn set_connect_delay(&mut self, delay: Duration) {
        self.connect_delay = delay;
    }

    pub fn set_write_timeout(&mut self, timeout: Duration) {
        self.write_timeout = timeout;
    }

    pub fn set_flush_interval(&mut self, interval: Duration) {
        self.flush_interval = interval;
    }

    pub fn set_retry_queue_len(&mut self, len: usize) {
        self.retry_queue_len = len;
    }

    pub(super) async fn new_connection(&self) -> anyhow::Result<FluentdConnection> {
        let socket = vey_socket::tcp::new_socket_to(
            self.server_addr.ip(),
            &self.bind,
            &self.tcp_keepalive,
            &Default::default(),
            false,
        )
        .map_err(|e| anyhow!("failed to setup socket: {e:?}"))?;
        let tcp_stream = socket
            .connect(self.server_addr)
            .await
            .map_err(|e| anyhow!("failed to tcp connect to peer {}: {e:?}", self.server_addr))?;

        if let Some(tls_client) = &self.tls_client {
            let default_tls_name = Host::Ip(self.server_addr.ip());
            let tls_name = self.tls_name.as_ref().unwrap_or(&default_tls_name);
            let ssl = tls_client
                .build_ssl(tls_name, self.server_addr.port())
                .map_err(|e| anyhow!("failed to prepare ssl: {e}"))?;
            let tls_connect = SslConnector::new(ssl, tcp_stream)
                .map_err(|e| anyhow!("failed to create TLS connector: {e}"))?;

            match tokio::time::timeout(tls_client.handshake_timeout, tls_connect.connect()).await {
                Ok(Ok(stream)) => Ok(FluentdConnection::Tls(stream)),
                Ok(Err(e)) => Err(anyhow!("failed to tls connect to peer: {e}")),
                Err(_) => Err(anyhow!("tls connect to peer timedout")),
            }
        } else {
            let tcp_stream = self
                .handshake(tcp_stream)
                .await
                .context("fluentd handshake failed")?;
            Ok(FluentdConnection::Tcp(tcp_stream))
        }
    }

    async fn handshake<T>(&self, mut connection: T) -> anyhow::Result<T>
    where
        T: AsyncRead + AsyncWrite + Unpin,
    {
        if self.shared_key.is_empty() {
            return Ok(connection);
        }

        let mut helo_buf = Vec::with_capacity(1024); // TODO config
        let helo_len = connection
            .read_buf(&mut helo_buf)
            .await
            .map_err(|e| anyhow!("failed to read helo msg: {e:?}"))?;
        let helo = super::handshake::parse_helo(&helo_buf[0..helo_len])
            .context("failed to parse helo msg")?;

        let mut shared_key_salt = [0u8; 16];
        openssl::rand::rand_bytes(&mut shared_key_salt)
            .map_err(|e| anyhow!("failed to generate shared key salt: {e}"))?;

        let ping_msg = self
            .build_ping(&helo, &shared_key_salt)
            .map_err(|e| anyhow!("failed to build ping msg: {e}"))?;
        connection
            .write_all(ping_msg.as_slice())
            .await
            .map_err(|e| anyhow!("failed to write ping msg: {e:?}"))?;
        connection
            .flush()
            .await
            .map_err(|e| anyhow!("failed to flush ping msg: {e:?}"))?;

        let mut pong_buf = Vec::with_capacity(1024); // TODO config
        let pong_len = connection
            .read_buf(&mut pong_buf)
            .await
            .map_err(|e| anyhow!("failed to read pong msg: {e:?}"))?;
        let pong = super::handshake::parse_pong(&pong_buf[0..pong_len])
            .context("failed to parse pong msg")?;

        self.verify_pong(pong, &shared_key_salt, helo.nonce)?;

        Ok(connection)
    }

    fn build_ping(
        &self,
        helo: &super::handshake::HeloMsgRef<'_>,
        shared_key_salt: &[u8],
    ) -> anyhow::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(1024);

        rmp::encode::write_array_len(&mut buf, 6)?;
        {
            rmp::encode::write_str(&mut buf, "PING")?;

            rmp::encode::write_str(&mut buf, &self.hostname)?;

            rmp::encode::write_bin(&mut buf, shared_key_salt)?;

            let mut md = MdCtx::new()?;
            md.digest_init(Md::sha512())?;
            md.digest_update(shared_key_salt)?;
            md.digest_update(self.hostname.as_bytes())?;
            md.digest_update(helo.nonce)?;
            md.digest_update(self.shared_key.as_bytes())?;
            let mut v = [0u8; FLUENTD_HASH_SIZE];
            md.digest_final(&mut v)?;
            let shared_key_digest = hex::encode(v);
            rmp::encode::write_str(&mut buf, &shared_key_digest)?;

            if helo.nonce.is_empty() {
                rmp::encode::write_str(&mut buf, "")?;

                rmp::encode::write_str(&mut buf, "")?;
            } else {
                rmp::encode::write_str(&mut buf, &self.username)?;

                md.digest_init(Md::sha512())?;
                md.digest_update(helo.auth_salt)?;
                md.digest_update(self.username.as_bytes())?;
                md.digest_update(self.password.as_bytes())?;
                md.digest_final(&mut v)?;
                let password_digest = hex::encode(v);
                rmp::encode::write_str(&mut buf, &password_digest)?;
            }
        }

        Ok(buf)
    }

    fn verify_pong(
        &self,
        pong: super::handshake::PongMsgRef,
        shared_key_salt: &[u8],
        nonce_salt: &[u8],
    ) -> anyhow::Result<()> {
        if !pong.auth_result {
            return Err(anyhow!("server auth failed, reason: {}", pong.reason));
        }

        let mut remote_hash = [0u8; FLUENTD_HASH_SIZE];
        hex::decode_to_slice(pong.shared_key_digest, &mut remote_hash)
            .map_err(|_| anyhow!("invalid shared_key_hex_digest returned by server"))?;

        let mut md = MdCtx::new()?;
        md.digest_init(Md::sha512())?;
        md.digest_update(shared_key_salt)?;
        md.digest_update(pong.server_hostname.as_bytes())?;
        md.digest_update(nonce_salt)?;
        md.digest_update(self.shared_key.as_bytes())?;
        let mut hash = [0u8; FLUENTD_HASH_SIZE];
        md.digest_final(&mut hash)?;

        if !constant_time_eq_64(&hash, &remote_hash) {
            return Err(anyhow!("shared_key_hex_digest mismatch"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::{PongMsgRef, parse_helo};
    use openssl::md::Md;
    use openssl::md_ctx::MdCtx;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::str::FromStr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    fn sha512_hex(parts: &[&[u8]]) -> String {
        let mut md = MdCtx::new().unwrap();
        md.digest_init(Md::sha512()).unwrap();
        for part in parts {
            md.digest_update(part).unwrap();
        }
        let mut hash = [0u8; FLUENTD_HASH_SIZE];
        md.digest_final(&mut hash).unwrap();
        hex::encode(hash)
    }

    #[test]
    fn defaults_and_setters() {
        let mut cfg = FluentdClientConfig::default();
        assert_eq!(
            cfg.server_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), FLUENTD_DEFAULT_PORT)
        );
        assert_eq!(cfg.connect_timeout, Duration::from_secs(10));
        assert_eq!(cfg.connect_delay, Duration::from_secs(10));
        assert_eq!(cfg.write_timeout, Duration::from_secs(1));
        assert_eq!(cfg.flush_interval, Duration::from_millis(100));
        assert_eq!(cfg.retry_queue_len, 10);
        assert!(cfg.shared_key.is_empty());
        assert!(cfg.tls_client.is_none());

        cfg.set_server_addr("10.0.0.1:24225".parse().unwrap());
        cfg.set_bind_ip("127.0.0.1".parse().unwrap());
        cfg.set_shared_key("sk".into());
        cfg.set_username("u".into());
        cfg.set_password("p".into());
        cfg.set_hostname("host".into());
        cfg.set_connect_timeout(Duration::from_secs(3));
        cfg.set_connect_delay(Duration::from_secs(4));
        cfg.set_write_timeout(Duration::from_millis(250));
        cfg.set_flush_interval(Duration::from_millis(50));
        cfg.set_retry_queue_len(7);
        cfg.set_tls_name(Host::from_str("example.com").unwrap());

        assert_eq!(cfg.server_addr, "10.0.0.1:24225".parse().unwrap());
        assert_eq!(cfg.bind.ip().unwrap().to_string(), "127.0.0.1");
        assert_eq!(cfg.shared_key, "sk");
        assert_eq!(cfg.username, "u");
        assert_eq!(cfg.password, "p");
        assert_eq!(cfg.hostname, "host");
        assert_eq!(cfg.connect_timeout, Duration::from_secs(3));
        assert_eq!(cfg.connect_delay, Duration::from_secs(4));
        assert_eq!(cfg.write_timeout, Duration::from_millis(250));
        assert_eq!(cfg.flush_interval, Duration::from_millis(50));
        assert_eq!(cfg.retry_queue_len, 7);
        assert!(cfg.tls_name.is_some());
    }

    #[tokio::test]
    async fn handshake_skipped_without_shared_key() {
        let cfg = FluentdClientConfig::default();
        let (client, _server) = duplex(64);
        cfg.handshake(client).await.unwrap();
    }

    #[test]
    fn build_ping_and_verify_pong_roundtrip() {
        let mut cfg = FluentdClientConfig::default();
        cfg.set_shared_key("secret".into());
        cfg.set_hostname("client-host".into());
        cfg.set_username("alice".into());
        cfg.set_password("pw".into());

        let helo = parse_helo(&[
            0x92, 0xa4, b'H', b'E', b'L', b'O', 0x82, 0xa5, b'n', b'o', b'n', b'c', b'e', 0xab,
            b'n', b'o', b'n', b'c', b'e', b'-', b'b', b'y', b't', b'e', b's', 0xa4, b'a', b'u',
            b't', b'h', 0xaa, b's', b'a', b'l', b't', b'-', b'b', b'y', b't', b'e', b's',
        ])
        .unwrap();
        assert_eq!(helo.nonce, b"nonce-bytes");
        assert_eq!(helo.auth_salt, b"salt-bytes");
        let shared_key_salt = b"0123456789abcdef";
        let ping = cfg.build_ping(&helo, shared_key_salt).unwrap();

        // PING is ["PING", hostname, salt, shared_key_digest, username, password_digest]
        let mut bytes = rmp::decode::Bytes::new(&ping);
        assert_eq!(rmp::decode::read_array_len(&mut bytes).unwrap(), 6);
        let (msg_type, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(msg_type, "PING");
        bytes = rmp::decode::Bytes::new(rest);
        let (hostname, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(hostname, "client-host");
        bytes = rmp::decode::Bytes::new(rest);
        let salt_len = rmp::decode::read_bin_len(&mut bytes).unwrap() as usize;
        assert_eq!(&bytes.remaining_slice()[..salt_len], shared_key_salt);
        bytes = rmp::decode::Bytes::new(&bytes.remaining_slice()[salt_len..]);
        let (client_digest, rest) =
            rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(
            client_digest,
            sha512_hex(&[shared_key_salt, b"client-host", b"nonce-bytes", b"secret"])
        );
        bytes = rmp::decode::Bytes::new(rest);
        let (username, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(username, "alice");
        bytes = rmp::decode::Bytes::new(rest);
        let (password_digest, _) =
            rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(
            password_digest,
            sha512_hex(&[b"salt-bytes", b"alice", b"pw"])
        );

        let server_hostname = "fluentd.example";
        let shared_key_digest = sha512_hex(&[
            shared_key_salt,
            server_hostname.as_bytes(),
            b"nonce-bytes",
            b"secret",
        ]);
        let pong = PongMsgRef {
            auth_result: true,
            reason: "",
            server_hostname,
            shared_key_digest: &shared_key_digest,
        };
        cfg.verify_pong(pong, shared_key_salt, b"nonce-bytes")
            .unwrap();
    }

    #[test]
    fn verify_pong_rejects_auth_failure_and_digest_mismatch() {
        let mut cfg = FluentdClientConfig::default();
        cfg.set_shared_key("secret".into());

        let failed = PongMsgRef {
            auth_result: false,
            reason: "bad key",
            server_hostname: "s",
            shared_key_digest: "",
        };
        assert!(
            cfg.verify_pong(failed, b"salt", b"nonce")
                .unwrap_err()
                .to_string()
                .contains("server auth failed")
        );

        let bad_hex = PongMsgRef {
            auth_result: true,
            reason: "",
            server_hostname: "s",
            shared_key_digest: "zz",
        };
        assert!(
            cfg.verify_pong(bad_hex, b"salt", b"nonce")
                .unwrap_err()
                .to_string()
                .contains("invalid shared_key_hex_digest")
        );

        let mismatch = PongMsgRef {
            auth_result: true,
            reason: "",
            server_hostname: "s",
            shared_key_digest: &"ab".repeat(64),
        };
        assert!(
            cfg.verify_pong(mismatch, b"salt", b"nonce")
                .unwrap_err()
                .to_string()
                .contains("mismatch")
        );
    }

    #[test]
    fn build_ping_omits_user_digest_when_nonce_empty() {
        let mut cfg = FluentdClientConfig::default();
        cfg.set_shared_key("secret".into());
        cfg.set_hostname("h".into());
        cfg.set_username("alice".into());
        cfg.set_password("pw".into());

        let helo = parse_helo(&[0x92, 0xa4, b'H', b'E', b'L', b'O', 0x80]).unwrap();
        assert_eq!(helo.nonce, b"");
        let ping = cfg.build_ping(&helo, b"salt0123456789ab").unwrap();
        let mut bytes = rmp::decode::Bytes::new(&ping);
        assert_eq!(rmp::decode::read_array_len(&mut bytes).unwrap(), 6);
        let (_, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        bytes = rmp::decode::Bytes::new(rest);
        let (_, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        bytes = rmp::decode::Bytes::new(rest);
        let salt_len = rmp::decode::read_bin_len(&mut bytes).unwrap() as usize;
        bytes = rmp::decode::Bytes::new(&bytes.remaining_slice()[salt_len..]);
        let (_, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        bytes = rmp::decode::Bytes::new(rest);
        let (username, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(username, "");
        bytes = rmp::decode::Bytes::new(rest);
        let (password_digest, _) =
            rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
        assert_eq!(password_digest, "");
    }

    #[tokio::test]
    async fn handshake_roundtrip_with_shared_key() {
        let mut cfg = FluentdClientConfig::default();
        cfg.set_shared_key("secret".into());
        cfg.set_hostname("client-host".into());
        cfg.set_username("alice".into());
        cfg.set_password("pw".into());

        let (client, mut server) = duplex(4096);
        let server_task = tokio::spawn(async move {
            // HELO with string nonce/auth
            let helo: &[u8] = &[
                0x92, 0xa4, b'H', b'E', b'L', b'O', 0x82, 0xa5, b'n', b'o', b'n', b'c', b'e', 0xa5,
                b'n', b'o', b'n', b'c', b'e', 0xa4, b'a', b'u', b't', b'h', 0xa4, b's', b'a', b'l',
                b't',
            ];
            assert_eq!(parse_helo(helo).unwrap().nonce, b"nonce");
            server.write_all(helo).await.unwrap();

            let mut buf = vec![0u8; 2048];
            let n = server.read(&mut buf).await.unwrap();
            let ping = &buf[..n];

            let mut bytes = rmp::decode::Bytes::new(ping);
            assert_eq!(rmp::decode::read_array_len(&mut bytes).unwrap(), 6);
            let (_, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
            bytes = rmp::decode::Bytes::new(rest);
            let (_, rest) = rmp::decode::read_str_from_slice(bytes.remaining_slice()).unwrap();
            bytes = rmp::decode::Bytes::new(rest);
            let salt_len = rmp::decode::read_bin_len(&mut bytes).unwrap() as usize;
            let shared_key_salt = bytes.remaining_slice()[..salt_len].to_vec();

            let server_hostname = "fluentd";
            let digest = sha512_hex(&[
                shared_key_salt.as_slice(),
                server_hostname.as_bytes(),
                b"nonce",
                b"secret",
            ]);

            let mut pong = Vec::new();
            rmp::encode::write_array_len(&mut pong, 5).unwrap();
            rmp::encode::write_str(&mut pong, "PONG").unwrap();
            rmp::encode::write_bool(&mut pong, true).unwrap();
            rmp::encode::write_str(&mut pong, "").unwrap();
            rmp::encode::write_str(&mut pong, server_hostname).unwrap();
            rmp::encode::write_str(&mut pong, &digest).unwrap();
            server.write_all(&pong).await.unwrap();
        });

        cfg.handshake(client).await.unwrap();
        server_task.await.unwrap();
    }
}
