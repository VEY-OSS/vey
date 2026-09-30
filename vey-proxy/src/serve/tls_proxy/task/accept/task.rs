/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use bytes::BytesMut;
use log::debug;
use openssl::ssl::Ssl;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::Instant;

use vey_codec::tls::{
    ClientHello, ExtensionType, HandshakeCoalescer, Record, RecordHeader, RecordParseError,
};
use vey_io_ext::OnceBufReader;
use vey_openssl::{SslAcceptor, SslStream};
use vey_types::net::{Host, TlsServerName};
use vey_types::route::HostMatch;

use super::super::{CommonTaskContext, TlsRelayTask};
use crate::audit::AuditContext;
use crate::config::server::ServerConfig;
use crate::serve::tls_proxy::TlsProxyHost;
use crate::serve::{
    ServerStats, ServerTaskError, ServerTaskForbiddenError, ServerTaskNotes, ServerTaskResult,
};
use crate::site::SiteContext;

const TLS_MAX_CLIENT_HELLO_SIZE: u32 = 1 << 16;

pub(crate) struct TlsAcceptTask {
    ctx: CommonTaskContext,
    hosts: Arc<HostMatch<Arc<TlsProxyHost>>>,
    audit_ctx: AuditContext,
    time_accepted: Instant,
}

impl TlsAcceptTask {
    pub(crate) fn new(
        ctx: CommonTaskContext,
        hosts: Arc<HostMatch<Arc<TlsProxyHost>>>,
        audit_ctx: AuditContext,
    ) -> Self {
        TlsAcceptTask {
            ctx,
            hosts,
            audit_ctx,
            time_accepted: Instant::now(),
        }
    }

    pub(crate) async fn into_running(self, stream: TcpStream) {
        self.pre_start();

        let local_addr = self.ctx.cc_info.sock_local_addr();
        let peer_addr = self.ctx.cc_info.sock_peer_addr();
        if let Err(e) = self.run(stream).await {
            debug!("{local_addr} - {peer_addr} tls accept error: {e}");
        }
    }

    fn pre_start(&self) {
        debug!(
            "new client from {} to {} server {}, using escaper {}",
            self.ctx.cc_info.client_addr(),
            self.ctx.server_config.r#type(),
            self.ctx.server_config.name(),
            self.ctx.server_config.escaper()
        );
    }

    async fn run(self, mut stream: TcpStream) -> ServerTaskResult<()> {
        let mut clt_r_buf = BytesMut::with_capacity(2048);
        let sni_host = tokio::time::timeout(
            self.ctx.server_config.client_hello_recv_timeout,
            self.read_sni_host(&mut stream, &mut clt_r_buf),
        )
        .await
        .map_err(|_| ServerTaskError::ClientAppTimeout("timeout to receive tls client hello"))??;
        let Some(req_host) = sni_host else {
            return Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::DestDenied,
            ));
        };
        let Some(host) = self.hosts.get(&req_host) else {
            return Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::DestDenied,
            ));
        };

        let site_ctx = SiteContext::new(
            Arc::clone(host.site()),
            Arc::clone(host.egress()),
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );
        if let Some(tenant) = site_ctx.tenant_ctx() {
            if tenant.is_expired() {
                return Err(ServerTaskError::ForbiddenByRule(
                    ServerTaskForbiddenError::UserBlocked,
                ));
            }
            if let Some(delay) = tenant.blocked_delay() {
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
                return Err(ServerTaskError::ForbiddenByRule(
                    ServerTaskForbiddenError::UserBlocked,
                ));
            }
        }

        let ssl_stream = self.accept_tls(host, stream, clt_r_buf).await?;
        let task_notes =
            ServerTaskNotes::new(self.ctx.cc_info.clone(), None, self.time_accepted.elapsed());
        TlsRelayTask::new(
            self.ctx,
            host.clone(),
            req_host,
            self.audit_ctx,
            task_notes,
            site_ctx,
        )
        .into_running(ssl_stream)
        .await;
        Ok(())
    }

    async fn accept_tls(
        &self,
        host: &TlsProxyHost,
        stream: TcpStream,
        clt_r_buf: BytesMut,
    ) -> ServerTaskResult<SslStream<OnceBufReader<TcpStream>>> {
        let tls_config = host.tls_server();
        let ssl = Ssl::new(&tls_config.ssl_context)
            .map_err(|_| ServerTaskError::InternalServerError("failed to create tls ssl"))?;
        let stream = OnceBufReader::new(stream, clt_r_buf);
        let ssl_acceptor = SslAcceptor::new(ssl, stream, tls_config.accept_timeout)
            .map_err(|_| ServerTaskError::InternalServerError("failed to create tls acceptor"))?;
        let ssl_stream = ssl_acceptor
            .accept()
            .await
            .map_err(|e| ServerTaskError::PeerTlsHandshakeFailed(anyhow!(e)))?;
        if ssl_stream.ssl().session_reused() {
            self.ctx.cc_info.tcp_sock_try_quick_ack();
        }
        Ok(ssl_stream)
    }

    async fn read_sni_host(
        &self,
        clt_r: &mut TcpStream,
        clt_r_buf: &mut BytesMut,
    ) -> ServerTaskResult<Option<Host>> {
        let max_hello_size = TLS_MAX_CLIENT_HELLO_SIZE as usize;
        let max_buf_size = max_hello_size
            .saturating_mul(RecordHeader::SIZE + 1)
            .saturating_add(1 << 14);
        let mut handshake_coalescer = HandshakeCoalescer::new(TLS_MAX_CLIENT_HELLO_SIZE);
        let mut record_offset = 0;
        loop {
            let mut record = match Record::parse(&clt_r_buf[record_offset..]) {
                Ok(r) => r,
                Err(RecordParseError::NeedMoreData(_)) => {
                    if clt_r_buf.len() >= max_buf_size {
                        return Err(ServerTaskError::InvalidClientProtocol(
                            "tls client hello message too large",
                        ));
                    }
                    match clt_r.read_buf(clt_r_buf).await {
                        Ok(0) => return Err(ServerTaskError::ClosedByClient),
                        Ok(_) => continue,
                        Err(e) => return Err(ServerTaskError::ClientTcpReadFailed(e)),
                    }
                }
                Err(_) => {
                    return Err(ServerTaskError::InvalidClientProtocol(
                        "invalid tls client hello request",
                    ));
                }
            };
            record_offset += record.encoded_len();

            match record.consume_handshake(&mut handshake_coalescer) {
                Ok(Some(handshake_msg)) => {
                    let ch = handshake_msg.parse_client_hello().map_err(|_| {
                        ServerTaskError::InvalidClientProtocol("invalid tls client hello request")
                    })?;
                    return Ok(self.host_from_client_hello(ch));
                }
                Ok(None) => match handshake_coalescer.parse_client_hello() {
                    Ok(Some(ch)) => return Ok(self.host_from_client_hello(ch)),
                    Ok(None) => {
                        if !record.consume_done() {
                            return Err(ServerTaskError::InvalidClientProtocol(
                                "partial fragmented tls client hello request",
                            ));
                        }
                    }
                    Err(_) => {
                        return Err(ServerTaskError::InvalidClientProtocol(
                            "invalid fragmented tls client hello request",
                        ));
                    }
                },
                Err(_) => {
                    return Err(ServerTaskError::InvalidClientProtocol(
                        "invalid tls client hello request",
                    ));
                }
            }
        }
    }

    fn host_from_client_hello(&self, ch: ClientHello<'_>) -> Option<Host> {
        match ch.get_ext(ExtensionType::ServerName) {
            Ok(Some(data)) => match TlsServerName::from_extension_value(data) {
                Ok(sni) => Some(Host::from(sni)),
                Err(_) => None,
            },
            Ok(None) | Err(_) => None,
        }
    }
}
