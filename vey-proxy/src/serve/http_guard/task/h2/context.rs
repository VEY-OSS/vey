/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::Bytes;
use h2::client::{ResponseFuture, SendRequest};
use h2::ext::Protocol;
use h2::server::SendResponse;
use h2::{Ping, PingPong, Reason, SendStream};
use http::{Request, Response, StatusCode, Version};
use tokio::sync::oneshot;
use uuid::Uuid;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_daemon::stat::task::TcpStreamTaskStats;
use vey_h2::RequestExt;
use vey_types::net::{
    AlpnProtocol, ForwardedValue, HeaderMapExt, Host, HttpForwardedHeaderType, UpstreamAddr,
};

use super::CommonTaskContext;
use crate::audit::AuditContext;
use crate::escape::{EgressNotes, TlsHttpConnection};
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpAliveReuseNotes,
    NilHttpForwardTaskRemoteStats,
};
use crate::module::http_header::{self, ProxyErrorType};
use crate::module::tcp_connect::{TcpConnectTaskConf, TcpConnection, TlsConnectTaskConf};
use crate::serve::{
    ServerTaskError, ServerTaskH2Error, ServerTaskNotes, ServerTaskResult, ServerTaskStage,
};
use crate::site::{H2ConnectionState, SiteContext};

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

// WebSocket origin is HTTP/2 only (TLS ALPN h2, or plaintext h2c).
// An ordinary stream negotiates: TLS offers h2 then http/1.1, plaintext is HTTP/1.1.
// `http.h2.force_upstream` keeps that stream on HTTP/2, so plaintext is h2c.
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

pub(crate) struct OpenedH2Stream {
    pub(crate) rsp_fut: ResponseFuture,
    pub(crate) send_stream: SendStream<Bytes>,
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

    pub(super) fn rsp_hdr_timeout(&self) -> Duration {
        self.site_ctx
            .rsp_hdr_recv_timeout()
            .unwrap_or(self.server_config.timeout.recv_rsp_header)
    }

    pub(super) fn append_forwarded<B>(&self, req: &mut Request<B>, host: Host) {
        let ty = self.site_ctx.site().forwarded_header_type();
        if matches!(ty, HttpForwardedHeaderType::Disable) {
            return;
        }
        if !self.site_ctx.site().trusts_forwarded_from(self.client_ip()) {
            req.headers_mut().strip_forwarded(ty);
        }
        let forwarded = ForwardedValue::from_client(self.client_addr(), self.forwarded_proto, host)
            .with_by(self.server_addr());
        req.headers_mut().append_forwarded(&forwarded, ty);
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
        upstream: &UpstreamAddr,
        req_host: &Host,
    ) -> ServerTaskResult<OriginConnection> {
        if let Some(origin) = self.checkout_h2(task_notes, upstream).await {
            return Ok(OriginConnection::H2(origin));
        }
        if self.site_ctx.site().config().http.h2.force_upstream {
            task_notes.stage = ServerTaskStage::Connecting;
            let origin = self
                .connect_origin_h2(task_notes, upstream, req_host)
                .await?;
            return Ok(OriginConnection::H2(origin));
        }
        if let Some(origin) = self.checkout_h1(task_notes, upstream).await {
            return Ok(OriginConnection::H1(origin));
        }
        task_notes.stage = ServerTaskStage::Connecting;
        self.connect_origin(task_notes, upstream, req_host).await
    }

    pub(super) async fn checkout_or_connect_h2(
        &self,
        task_notes: &mut ServerTaskNotes,
        upstream: &UpstreamAddr,
        req_host: &Host,
    ) -> ServerTaskResult<OriginH2Sender> {
        if let Some(origin) = self.checkout_h2(task_notes, upstream).await {
            return Ok(origin);
        }
        task_notes.stage = ServerTaskStage::Connecting;
        self.connect_origin_h2(task_notes, upstream, req_host).await
    }

    /// `ready` on a pooled sender can still fail after checkout. Open a new
    /// connection to the same upstream instead of failing the client stream.
    pub(super) async fn ready_h2_sender(
        &self,
        task_notes: &mut ServerTaskNotes,
        upstream: &UpstreamAddr,
        req_host: &Host,
        origin: OriginH2Sender,
    ) -> ServerTaskResult<OriginH2Sender> {
        let open_timeout = self.server_config.h2.upstream_stream_open_timeout;
        let reused = origin.reused;
        let egress_notes = origin.egress_notes;
        match tokio::time::timeout(open_timeout, origin.sender.ready()).await {
            Ok(Ok(sender)) => {
                return Ok(OriginH2Sender {
                    sender,
                    reused,
                    egress_notes,
                });
            }
            Ok(Err(e)) => {
                if !reused {
                    return Err(ServerTaskError::H2(
                        ServerTaskH2Error::UpstreamStreamOpenFailed(e),
                    ));
                }
            }
            Err(_) => {
                if !reused {
                    return Err(ServerTaskError::H2(
                        ServerTaskH2Error::UpstreamStreamOpenTimeout,
                    ));
                }
            }
        }

        task_notes.stage = ServerTaskStage::Connecting;
        let origin = self
            .connect_origin_h2(task_notes, upstream, req_host)
            .await?;
        let egress_notes = origin.egress_notes;
        match tokio::time::timeout(open_timeout, origin.sender.ready()).await {
            Ok(Ok(sender)) => Ok(OriginH2Sender {
                sender,
                reused: false,
                egress_notes,
            }),
            Ok(Err(e)) => Err(ServerTaskError::H2(
                ServerTaskH2Error::UpstreamStreamOpenFailed(e),
            )),
            Err(_) => Err(ServerTaskError::H2(
                ServerTaskH2Error::UpstreamStreamOpenTimeout,
            )),
        }
    }

    /// Send the request head and wait until the stream is really opened.
    ///
    /// `ready` on a fresh `SendRequest` clone does not wait for
    /// MAX_CONCURRENT_STREAMS; only `poll_ready` after `send_request` on the
    /// same handle does. A pooled connection that fails or stays full is
    /// replaced by a new one, which is safe as no request body is sent yet.
    pub(super) async fn open_h2_stream(
        &self,
        task_notes: &mut ServerTaskNotes,
        upstream: &UpstreamAddr,
        req_host: &Host,
        origin: OriginH2Sender,
        req: &Request<()>,
        end_of_stream: bool,
    ) -> ServerTaskResult<OpenedH2Stream> {
        let reused = origin.reused;
        match self.try_open_h2_stream(origin, req, end_of_stream).await {
            Ok(opened) => return Ok(opened),
            Err(_) if reused => {}
            Err(e) => return Err(e),
        }

        task_notes.stage = ServerTaskStage::Connecting;
        let origin = self
            .connect_origin_h2(task_notes, upstream, req_host)
            .await?;
        self.try_open_h2_stream(origin, req, end_of_stream).await
    }

    async fn try_open_h2_stream(
        &self,
        origin: OriginH2Sender,
        req: &Request<()>,
        end_of_stream: bool,
    ) -> ServerTaskResult<OpenedH2Stream> {
        let OriginH2Sender {
            mut sender,
            reused,
            egress_notes,
        } = origin;
        // clone_header drops extensions. The client request also carries this
        // connection's StreamId; only Protocol must be forwarded, as the
        // upstream :protocol pseudo-header.
        let mut ups_req = req.clone_header();
        if let Some(protocol) = req.extensions().get::<Protocol>() {
            ups_req.extensions_mut().insert(protocol.clone());
        }
        let (rsp_fut, send_stream) = sender.send_request(ups_req, end_of_stream).map_err(|e| {
            ServerTaskError::UpstreamAppError(anyhow::anyhow!(
                "send h2 request to upstream failed: {e}"
            ))
        })?;
        match tokio::time::timeout(
            self.server_config.h2.upstream_stream_open_timeout,
            poll_fn(|cx| sender.poll_ready(cx)),
        )
        .await
        {
            Ok(Ok(())) => Ok(OpenedH2Stream {
                rsp_fut,
                send_stream,
                reused,
                egress_notes,
            }),
            Ok(Err(e)) => Err(ServerTaskError::H2(
                ServerTaskH2Error::UpstreamStreamOpenFailed(e),
            )),
            Err(_) => Err(ServerTaskError::H2(
                ServerTaskH2Error::UpstreamStreamOpenTimeout,
            )),
        }
    }

    pub(super) fn reset_unopened_stream(
        clt_send_rsp: &mut SendResponse<Bytes>,
        err: &ServerTaskError,
    ) {
        let reason = match err {
            ServerTaskError::H2(ServerTaskH2Error::UpstreamStreamOpenFailed(e)) => {
                e.reason().unwrap_or(Reason::REFUSED_STREAM)
            }
            ServerTaskError::H2(ServerTaskH2Error::UpstreamStreamOpenTimeout) => {
                Reason::REFUSED_STREAM
            }
            _ => return,
        };
        clt_send_rsp.send_reset(reason);
    }

    async fn checkout_h2(
        &self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
    ) -> Option<OriginH2Sender> {
        let open_timeout = self.server_config.h2.upstream_stream_open_timeout;
        let (sender, egress_notes) = self
            .site_ctx
            .site()
            .http2_pool()
            .checkout(
                task_notes.worker_id(),
                self.escaper.name(),
                upstream.socket_addr(),
                open_timeout,
            )
            .await?;
        Some(OriginH2Sender {
            sender,
            reused: true,
            egress_notes,
        })
    }

    async fn checkout_h1(
        &self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
    ) -> Option<OriginH1Sender> {
        let site = self.site_ctx.site();
        let keepalive = site.h1_keepalive_config();
        if !keepalive.is_enabled() {
            return None;
        }
        let pool = site.http1_pool()?;
        let (connection, reuse_notes, egress_notes) = pool
            .get(
                task_notes.worker_id(),
                self.escaper.name(),
                upstream.socket_addr(),
            )
            .await?;
        let task_stats: ArcHttpForwardTaskRemoteStats = Arc::new(NilHttpForwardTaskRemoteStats);
        let connection = reuse_notes
            .escaper
            .prepare_reused_http_forward_connection(connection, task_notes, task_stats);
        Some(OriginH1Sender {
            connection,
            reused: true,
            reuse_notes,
            egress_notes,
        })
    }

    pub(super) async fn connect_origin_h1(
        &self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        req_host: &Host,
    ) -> ServerTaskResult<OriginH1Sender> {
        let mut fwd_ctx = self
            .escaper
            .new_http_forward_context(Arc::clone(&self.escaper));
        let mut audit_ctx = AuditContext::new(self.audit_handle.clone());
        let task_stats: ArcHttpForwardTaskRemoteStats = Arc::new(NilHttpForwardTaskRemoteStats);
        let site = self.site_ctx.site();
        let _ = fwd_ctx
            .check_in_final_escaper(task_notes, upstream, site.tls_client().is_some())
            .await;
        let (connection, reuse_notes) = if let Some(tls_client) = site.tls_client() {
            let task_conf = TlsConnectTaskConf {
                tcp: TcpConnectTaskConf { upstream },
                tls_config: tls_client,
                tls_name: site.tls_name_or(req_host),
                alpn_protocols: None,
            };
            fwd_ctx
                .new_prepared_https_connection(&task_conf, task_notes, task_stats, &mut audit_ctx)
                .await?
        } else {
            let task_conf = TcpConnectTaskConf { upstream };
            fwd_ctx
                .new_prepared_http_connection(&task_conf, task_notes, task_stats, &mut audit_ctx)
                .await?
        };

        let mut egress_notes = EgressNotes::default();
        fwd_ctx.fetch_egress_notes(&mut egress_notes);
        Ok(OriginH1Sender {
            connection,
            reused: false,
            reuse_notes,
            egress_notes,
        })
    }

    async fn connect_origin(
        &self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        req_host: &Host,
    ) -> ServerTaskResult<OriginConnection> {
        let site = self.site_ctx.site();
        match site.tls_client() {
            Some(tls_config) => {
                let mut egress_notes = EgressNotes::default();
                let mut audit_ctx = AuditContext::new(self.audit_handle.clone());
                let task_conf = TlsConnectTaskConf {
                    tcp: TcpConnectTaskConf { upstream },
                    tls_config,
                    tls_name: site.tls_name_or(req_host),
                    alpn_protocols: Some(ORIGIN_TLS_ALPN_H2_H1),
                };
                match self
                    .escaper
                    .tls_setup_http_connection(
                        Arc::clone(&self.escaper),
                        &task_conf,
                        &mut egress_notes,
                        task_notes,
                        &mut audit_ctx,
                    )
                    .await?
                {
                    TlsHttpConnection::H1(connection, escaper) => {
                        Ok(OriginConnection::H1(OriginH1Sender {
                            connection,
                            reused: false,
                            reuse_notes: HttpAliveReuseNotes::from_new(escaper),
                            egress_notes,
                        }))
                    }
                    TlsHttpConnection::H2(ups_c) => {
                        let h2_sender = self
                            .finish_h2_origin(ups_c, egress_notes, task_notes, upstream)
                            .await?;
                        Ok(OriginConnection::H2(h2_sender))
                    }
                }
            }
            None => {
                let origin = self
                    .connect_origin_h1(task_notes, upstream, req_host)
                    .await?;
                Ok(OriginConnection::H1(origin))
            }
        }
    }

    async fn connect_origin_h2(
        &self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        req_host: &Host,
    ) -> ServerTaskResult<OriginH2Sender> {
        let site = self.site_ctx.site();
        let mut egress_notes = EgressNotes::default();
        let mut audit_ctx = AuditContext::new(self.audit_handle.clone());
        let task_stats: ArcTcpConnectionTaskRemoteStats = Arc::new(TcpStreamTaskStats::default());

        let ups_c = if let Some(tls_config) = site.tls_client() {
            let task_conf = TlsConnectTaskConf {
                tcp: TcpConnectTaskConf { upstream },
                tls_config,
                tls_name: site.tls_name_or(req_host),
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
                .await?
        } else {
            let task_conf = TcpConnectTaskConf { upstream };
            self.escaper
                .tcp_setup_connection(
                    &task_conf,
                    &mut egress_notes,
                    task_notes,
                    task_stats,
                    &mut audit_ctx,
                )
                .await?
        };

        self.finish_h2_origin(ups_c, egress_notes, task_notes, upstream)
            .await
    }

    async fn finish_h2_origin(
        &self,
        ups_c: TcpConnection,
        egress_notes: EgressNotes,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
    ) -> ServerTaskResult<OriginH2Sender> {
        let (sender, conn_state) = self.handshake_h2(ups_c).await?;
        self.site_ctx.site().http2_pool().insert(
            task_notes.worker_id(),
            self.escaper.name().clone(),
            upstream.socket_addr(),
            sender.clone(),
            conn_state,
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
        ups_c: TcpConnection,
    ) -> ServerTaskResult<(SendRequest<Bytes>, Arc<H2ConnectionState>)> {
        let (ups_r, ups_w) = ups_c;
        let client_builder = self.server_config.h2.build_client();
        let (sender, mut connection) = tokio::time::timeout(
            self.server_config.h2.upstream_handshake_timeout,
            client_builder.handshake(tokio::io::join(ups_r, ups_w)),
        )
        .await
        .map_err(|_| ServerTaskError::UpstreamAppTimeout("upstream h2 handshake timeout"))?
        .map_err(|e| {
            ServerTaskError::UpstreamAppError(anyhow::anyhow!("upstream h2 handshake failed: {e}"))
        })?;

        let conn_state = Arc::new(H2ConnectionState::new());
        let Some(ping) = connection.ping_pong() else {
            unreachable!()
        };
        let ping_quit_tx = self.spawn_origin_ping(ping, conn_state.clone());

        let conn_state2 = conn_state.clone();
        tokio::spawn(async move {
            let _ = connection.await;
            conn_state2.mark_closed();
            let _ = ping_quit_tx.send(());
        });

        Ok((sender, conn_state))
    }

    fn spawn_origin_ping(
        &self,
        mut ping: PingPong,
        state: Arc<H2ConnectionState>,
    ) -> oneshot::Sender<()> {
        let (ping_quit_tx, mut ping_quit_rx) = oneshot::channel();
        let ping_interval = self.site_ctx.site().config().http.h2.ping_interval;
        if ping_interval.is_zero() {
            return ping_quit_tx;
        }
        let ping_timeout = self.site_ctx.site().config().http.h2.ping_timeout;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(ping_interval);
            loop {
                tokio::select! {
                    _ = &mut ping_quit_rx => break,
                    _ = ticker.tick() => {}
                }

                match tokio::time::timeout(ping_timeout, ping.ping(Ping::opaque())).await {
                    Ok(Ok(_)) => continue,
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            state.mark_closed();
        });
        ping_quit_tx
    }
}
