/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::anyhow;
use bytes::Bytes;
use h2::Ping;
use h2::client::SendRequest;
use h2::server::SendResponse;
use http::{Request, Response, StatusCode, Version};
use tokio::sync::oneshot;
use uuid::Uuid;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_daemon::stat::task::TcpStreamTaskStats;
use vey_h2::RequestExt;
use vey_types::net::{AlpnProtocol, ForwardedValue, Host, HttpForwardedHeaderType};

use super::{CommonTaskContext, H2StreamTransferError};
use crate::audit::AuditContext;
use crate::escape::EgressNotes;
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpAliveReuseNotes,
    NilHttpForwardTaskRemoteStats,
};
use crate::module::http_header::{self, ProxyErrorType};
use crate::module::tcp_connect::{TcpConnectTaskConf, TcpConnection, TlsConnectTaskConf};
use crate::serve::{ServerTaskNotes, ServerTaskStage};
use crate::site::SiteContext;

pub(crate) struct H2TaskContext {
    pub(crate) common: CommonTaskContext,
    pub(crate) site_ctx: SiteContext,
    pub(crate) connection_id: Uuid,
    tenant_conn_counted: AtomicBool,
}

impl Deref for H2TaskContext {
    type Target = CommonTaskContext;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

// WebSocket origin is H2-only. Normal H2 clients offer both and upgrade by ALPN.
const ORIGIN_TLS_ALPN_H2: &[AlpnProtocol] = &[AlpnProtocol::Http2];
const ORIGIN_TLS_ALPN_H2_H1: &[AlpnProtocol] = &[AlpnProtocol::Http2, AlpnProtocol::Http11];

pub(crate) enum OriginConnection {
    H2(OriginH2Sender),
    H1(OriginH1Sender),
}

pub(crate) struct OriginH2Sender {
    pub(crate) sender: SendRequest<Bytes>,
    pub(crate) reused: bool,
    pub(crate) egress_notes: EgressNotes,
}

pub(crate) struct OriginH1Sender {
    pub(crate) connection: BoxHttpForwardConnection,
    pub(crate) reused: bool,
    pub(crate) reuse_notes: HttpAliveReuseNotes,
    pub(crate) egress_notes: EgressNotes,
}

impl H2TaskContext {
    pub(crate) fn new(
        common: CommonTaskContext,
        site_ctx: SiteContext,
        connection_id: Uuid,
    ) -> Self {
        H2TaskContext {
            common,
            site_ctx,
            connection_id,
            tenant_conn_counted: AtomicBool::new(false),
        }
    }

    pub(crate) fn site_ctx_for_request(&self) -> SiteContext {
        let mut site_ctx = self.site_ctx.clone();
        if self.tenant_conn_counted.swap(true, Ordering::Relaxed) {
            site_ctx.mark_reused_client_connection();
        }
        site_ctx
    }

    pub(super) fn append_forwarded<B>(&self, req: &mut Request<B>) {
        let ty = self.site_ctx.site().forwarded_header_type();
        if matches!(ty, HttpForwardedHeaderType::Disable) {
            return;
        }
        let host = req.host();
        if !self.site_ctx.site().trusts_forwarded_from(self.client_ip()) {
            ForwardedValue::strip_http(req.headers_mut(), ty);
        }
        ForwardedValue::from_client(self.client_addr(), self.forwarded_proto, &host)
            .append_to_http(req.headers_mut(), ty, self.server_addr());
    }

    pub(super) fn local_error_response(
        &self,
        status: StatusCode,
        error: ProxyErrorType,
    ) -> Option<Response<()>> {
        if self.server_config.no_proxy_status {
            return Response::builder()
                .status(status)
                .version(Version::HTTP_2)
                .body(())
                .ok();
        }
        let ident = self
            .server_config
            .server_id
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(http_header::DEFAULT_PROXY_STATUS_IDENT);
        Response::builder()
            .status(status)
            .version(Version::HTTP_2)
            .header(
                "proxy-status",
                http_header::proxy_status_value(ident, error),
            )
            .body(())
            .ok()
    }

    pub(super) fn reply_early_error(
        &self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        status: StatusCode,
        error: ProxyErrorType,
    ) {
        let _ = self.reply_local_error(clt_send_rsp, status, error);
    }

    pub(super) fn reply_local_error(
        &self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        status: StatusCode,
        error: ProxyErrorType,
    ) -> Option<u16> {
        let rsp = self.local_error_response(status, error)?;
        let rsp_status = rsp.status().as_u16();
        clt_send_rsp.send_response(rsp, true).ok()?;
        Some(rsp_status)
    }

    pub(super) async fn checkout_or_connect(
        &self,
        task_notes: &mut ServerTaskNotes,
        request_host: &Host,
    ) -> Result<OriginConnection, H2StreamTransferError> {
        if let Some(origin) = self.checkout_h2(task_notes).await {
            return Ok(OriginConnection::H2(origin));
        }
        if let Some(origin) = self.checkout_h1(task_notes).await {
            return Ok(OriginConnection::H1(origin));
        }
        task_notes.stage = ServerTaskStage::Connecting;
        self.connect_origin(task_notes, request_host).await
    }

    pub(super) async fn checkout_or_connect_h2(
        &self,
        task_notes: &mut ServerTaskNotes,
        request_host: &Host,
    ) -> Result<OriginH2Sender, H2StreamTransferError> {
        if let Some(origin) = self.checkout_h2(task_notes).await {
            return Ok(origin);
        }
        task_notes.stage = ServerTaskStage::Connecting;
        self.connect_origin_h2(task_notes, request_host).await
    }

    async fn checkout_h2(&self, task_notes: &ServerTaskNotes) -> Option<OriginH2Sender> {
        let open_timeout = self.server_config.h2.upstream_stream_open_timeout;
        let peer = task_notes.site_upstream_peer();
        let (sender, egress_notes) = self
            .site_ctx
            .site()
            .http2_pool()
            .checkout(
                task_notes.worker_id(),
                self.escaper.name(),
                peer,
                open_timeout,
            )
            .await?;
        Some(OriginH2Sender {
            sender,
            reused: true,
            egress_notes,
        })
    }

    async fn checkout_h1(&self, task_notes: &ServerTaskNotes) -> Option<OriginH1Sender> {
        let site = self.site_ctx.site();
        let keepalive = site.h1_keepalive_config();
        if !keepalive.is_enabled() {
            return None;
        }
        let pool = site.http1_pool()?;
        let peer = task_notes.site_upstream_peer();
        let (connection, reuse_notes, egress_notes) = pool
            .get(task_notes.worker_id(), self.escaper.name(), peer)
            .await?;
        let task_stats: ArcHttpForwardTaskRemoteStats = Arc::new(NilHttpForwardTaskRemoteStats);
        let connection = reuse_notes.escaper.prepare_reused_http_forward_connection(
            connection,
            task_notes,
            task_stats,
            site.tls_client().is_some(),
        );
        Some(OriginH1Sender {
            connection,
            reused: true,
            reuse_notes,
            egress_notes,
        })
    }

    async fn connect_origin(
        &self,
        task_notes: &ServerTaskNotes,
        request_host: &Host,
    ) -> Result<OriginConnection, H2StreamTransferError> {
        let site = self.site_ctx.site();
        let upstream = task_notes
            .site_upstream()
            .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))?;
        let mut egress_notes = EgressNotes::default();
        let mut audit_ctx = AuditContext::new(self.audit_handle.clone());
        let task_stats: ArcTcpConnectionTaskRemoteStats = Arc::new(TcpStreamTaskStats::default());

        let stream = if let Some(tls_client) = site.tls_client() {
            let task_conf = TlsConnectTaskConf {
                tcp: TcpConnectTaskConf { upstream },
                tls_config: tls_client,
                tls_name: site.tls_name_or(request_host),
                alpn_protocols: Some(ORIGIN_TLS_ALPN_H2_H1),
            };
            let (stream, leaf) = self
                .escaper
                .tls_connect(&task_conf, &mut egress_notes, task_notes, &mut audit_ctx)
                .await
                .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))?;
            let wrap_escaper = leaf.unwrap_or_else(|| self.escaper.clone());
            if egress_notes.selected_alpn != Some(AlpnProtocol::Http2) {
                let h1_stats: ArcHttpForwardTaskRemoteStats =
                    Arc::new(NilHttpForwardTaskRemoteStats);
                return Ok(OriginConnection::H1(OriginH1Sender {
                    connection: wrap_escaper
                        .http_forward_from_tls_connection(stream, task_notes, h1_stats),
                    reused: false,
                    reuse_notes: HttpAliveReuseNotes::from_new(wrap_escaper),
                    egress_notes,
                }));
            }
            wrap_escaper.tls_connection_with_task_stats(stream, task_notes, task_stats)
        } else {
            self.setup_origin_tcp(task_notes, &mut egress_notes, &mut audit_ctx, task_stats)
                .await?
        };

        Ok(OriginConnection::H2(
            self.finish_h2_origin(stream, egress_notes, task_notes)
                .await?,
        ))
    }

    async fn connect_origin_h2(
        &self,
        task_notes: &ServerTaskNotes,
        request_host: &Host,
    ) -> Result<OriginH2Sender, H2StreamTransferError> {
        let site = self.site_ctx.site();
        let upstream = task_notes
            .site_upstream()
            .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))?;
        let mut egress_notes = EgressNotes::default();
        let mut audit_ctx = AuditContext::new(self.audit_handle.clone());
        let task_stats: ArcTcpConnectionTaskRemoteStats = Arc::new(TcpStreamTaskStats::default());

        let stream = if let Some(tls_client) = site.tls_client() {
            let task_conf = TlsConnectTaskConf {
                tcp: TcpConnectTaskConf { upstream },
                tls_config: tls_client,
                tls_name: site.tls_name_or(request_host),
                alpn_protocols: Some(ORIGIN_TLS_ALPN_H2),
            };
            self.escaper
                .tls_setup_connection(
                    &task_conf,
                    &mut egress_notes,
                    task_notes,
                    task_stats,
                    &mut audit_ctx,
                )
                .await
                .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))?
        } else {
            self.setup_origin_tcp(task_notes, &mut egress_notes, &mut audit_ctx, task_stats)
                .await?
        };

        self.finish_h2_origin(stream, egress_notes, task_notes)
            .await
    }

    async fn setup_origin_tcp(
        &self,
        task_notes: &ServerTaskNotes,
        egress_notes: &mut EgressNotes,
        audit_ctx: &mut AuditContext,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> Result<TcpConnection, H2StreamTransferError> {
        let upstream = task_notes
            .site_upstream()
            .map_err(H2StreamTransferError::OriginConnectFailed)?;
        let task_conf = TcpConnectTaskConf { upstream };
        self.escaper
            .tcp_setup_connection(&task_conf, egress_notes, task_notes, task_stats, audit_ctx)
            .await
            .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))
    }

    async fn finish_h2_origin(
        &self,
        stream: TcpConnection,
        egress_notes: EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<OriginH2Sender, H2StreamTransferError> {
        let (sender, closed) = self.handshake_h2(stream).await?;
        self.site_ctx.site().http2_pool().insert(
            task_notes.worker_id(),
            self.escaper.name().clone(),
            task_notes.site_upstream_peer(),
            sender.clone(),
            Arc::clone(&closed),
            egress_notes.clone(),
        );
        Ok(OriginH2Sender {
            sender,
            reused: false,
            egress_notes,
        })
    }

    async fn handshake_h2(
        &self,
        stream: TcpConnection,
    ) -> Result<(SendRequest<Bytes>, Arc<AtomicBool>), H2StreamTransferError> {
        let (ups_r, ups_w) = stream;
        let mut client_builder = h2::client::Builder::new();
        self.server_config
            .h2
            .apply_to_client_builder(&mut client_builder);

        let (sender, mut connection) = tokio::time::timeout(
            self.server_config.h2.upstream_handshake_timeout,
            client_builder.handshake(tokio::io::join(ups_r, ups_w)),
        )
        .await
        .map_err(|_| {
            H2StreamTransferError::OriginConnectFailed(anyhow!("upstream h2 handshake timeout"))
        })?
        .map_err(|e| {
            H2StreamTransferError::OriginConnectFailed(anyhow!("upstream h2 handshake: {e}"))
        })?;

        let closed = Arc::new(AtomicBool::new(false));
        let (ping_quit_tx, ping_fail_rx) = self.spawn_origin_ping(&mut connection, &closed);

        let closed_flag = Arc::clone(&closed);
        tokio::spawn(async move {
            if let Some(ping_fail_rx) = ping_fail_rx {
                tokio::select! {
                    _ = connection => {
                        let _ = ping_quit_tx.send(());
                    }
                    _ = ping_fail_rx => {}
                }
            } else {
                let _ = connection.await;
                let _ = ping_quit_tx.send(());
            }
            closed_flag.store(true, Ordering::Release);
        });

        Ok((sender, closed))
    }

    fn spawn_origin_ping<T, B>(
        &self,
        connection: &mut h2::client::Connection<T, B>,
        closed: &Arc<AtomicBool>,
    ) -> (oneshot::Sender<()>, Option<oneshot::Receiver<()>>)
    where
        T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
        B: bytes::Buf,
    {
        let (ping_quit_tx, mut ping_quit_rx) = oneshot::channel();
        let ping_interval = self.server_config.h2.ping_interval;
        if !ping_interval.is_zero()
            && let Some(mut ping) = connection.ping_pong()
        {
            let closed = Arc::clone(closed);
            let (ping_fail_tx, ping_fail_rx) = oneshot::channel();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(ping_interval);
                loop {
                    tokio::select! {
                        _ = &mut ping_quit_rx => break,
                        _ = ticker.tick() => {
                            if ping.ping(Ping::opaque()).await.is_err() {
                                closed.store(true, Ordering::Release);
                                let _ = ping_fail_tx.send(());
                                break;
                            }
                        }
                    }
                }
            });
            return (ping_quit_tx, Some(ping_fail_rx));
        }
        (ping_quit_tx, None)
    }
}
