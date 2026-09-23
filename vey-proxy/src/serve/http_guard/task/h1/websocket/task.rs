/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use bytes::Bytes;
use http::header;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_daemon::server::ServerQuitPolicy;
use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_http::client::HttpForwardRemoteResponse;
use vey_http::server::HttpProxyClientRequest;
use vey_http::{HttpBodyReader, HttpBodyType};
use vey_icap_client::reqmod::IcapReqmodClient;
use vey_icap_client::reqmod::h1::{
    HttpAdapterErrorResponse, HttpRequestAdapter, ReqmodAdaptationMidState,
    ReqmodAdaptationRunState, ReqmodRecvHttpResponseBody,
};
use vey_io_ext::{
    FlexBufReader, GlobalLimitGroup, IdleInterval, LimitedReader, LimitedWriteExt, OnceBufReader,
    StreamCopy, StreamCopyConfig, StreamCopyError,
};
use vey_types::acl::AclAction;
use vey_types::net::{TcpSockSpeedLimitConfig, UpstreamAddr};

use super::H1TaskContext;
use super::protocol::{HttpClientReader, HttpClientWriter, HttpGuardRequest};
use crate::audit::AuditContext;
use crate::auth::User;
use crate::config::server::ServerConfig;
use crate::escape::EgressNotes;
use crate::inspect::StreamTransitTask;
use crate::log::task::websocket::TaskLogForWebSocket;
use crate::module::http_forward::{HttpProxyClientResponse, send_req_header_to_origin};
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectTaskConf, TcpConnection, TlsConnectTaskConf,
};
use crate::module::websocket::{WebSocketTaskNotes, WebSocketTaskStats};
use crate::serve::http_guard::HttpForwardTaskAliveGuard;
use crate::serve::{
    ServerIdleChecker, ServerStats, ServerTaskError, ServerTaskForbiddenError, ServerTaskNotes,
    ServerTaskResult, ServerTaskStage,
};
use crate::site::SiteContext;
use crate::stat::types::RequestAliveKind;

use super::stats::WebSocketTaskCltWrapperStats;

pub(crate) struct HttpGuardWebsocketTask {
    ctx: Arc<H1TaskContext>,
    site_ctx: SiteContext,
    task_notes: ServerTaskNotes,
    ws_notes: WebSocketTaskNotes,
    egress_notes: EgressNotes,
    task_stats: Arc<WebSocketTaskStats>,
    max_idle_count: usize,
    ups_r_leftover: Option<Bytes>,
    send_error_response: bool,
    _alive_guard: Option<HttpForwardTaskAliveGuard>,
    upstream: UpstreamAddr,
}

impl HttpGuardWebsocketTask {
    pub(crate) fn new(
        ctx: &Arc<H1TaskContext>,
        req: &HttpGuardRequest<impl AsyncRead>,
        site_ctx: SiteContext,
        task_notes: ServerTaskNotes,
    ) -> Self {
        let uri_log_max_chars = site_ctx
            .log_uri_max_chars()
            .unwrap_or(ctx.server_config.log_uri_max_chars);
        let ws_notes =
            WebSocketTaskNotes::new(req.inner.version, req.inner.uri.clone(), uri_log_max_chars);
        let max_idle_count = task_notes.task_max_idle_count(ctx.server_config.task_idle_max_count);
        let upstream = task_notes.site_upstream_addr().clone();
        HttpGuardWebsocketTask {
            ctx: Arc::clone(ctx),
            site_ctx,
            task_notes,
            ws_notes,
            egress_notes: EgressNotes::default(),
            task_stats: Arc::new(WebSocketTaskStats::default()),
            max_idle_count,
            ups_r_leftover: None,
            send_error_response: true,
            _alive_guard: None,
            upstream,
        }
    }

    pub(crate) async fn connect_to_origin<CDR, CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_r: &mut HttpClientReader<CDR>,
        clt_w: &mut HttpClientWriter<CDW>,
    ) -> Option<(TcpConnection, HttpForwardRemoteResponse)>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        self.pre_start();
        match self.do_connect(req, clt_r, clt_w).await {
            Ok(connected) => {
                if connected.is_none()
                    && let Some(log_ctx) = self.get_log_context()
                {
                    log_ctx.log(&ServerTaskError::Finished);
                }
                connected
            }
            Err(e) => {
                if self.send_error_response {
                    self.reply_task_err(req, &e, clt_w).await;
                }
                if let Some(log_ctx) = self.get_log_context() {
                    log_ctx.log(&e);
                }
                None
            }
        }
    }

    pub(crate) async fn into_running<CDR, CDW>(
        mut self,
        clt_r: HttpClientReader<CDR>,
        clt_w: HttpClientWriter<CDW>,
        ups_c: TcpConnection,
        rsp: HttpForwardRemoteResponse,
    ) where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        let e = match self.run_connected(clt_r, clt_w, ups_c, rsp).await {
            Ok(()) => ServerTaskError::Finished,
            Err(e) => e,
        };
        if let Some(log_ctx) = self.get_log_context() {
            log_ctx.log(&e);
        }
    }

    fn get_log_context(&self) -> Option<TaskLogForWebSocket<'_>> {
        let Some(logger) = &self.ctx.task_logger else {
            return None;
        };
        Some(TaskLogForWebSocket {
            logger,
            upstream: &self.upstream,
            task_notes: &self.task_notes,
            ws_notes: &self.ws_notes,
            egress_notes: &self.egress_notes,
            client_rd_bytes: self.task_stats.clt.read.get_bytes(),
            client_wr_bytes: self.task_stats.clt.write.get_bytes(),
            remote_rd_bytes: self.task_stats.ups.read.get_bytes(),
            remote_wr_bytes: self.task_stats.ups.write.get_bytes(),
            clt_stream_id: None,
            ups_stream_id: None,
            connection_id: None,
        })
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_forward_task());
        self.task_notes.hold_req_alive(RequestAliveKind::Websocket);
        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_created();
        }
    }

    fn enable_custom_header_for_local_reply(&self, rsp: &mut HttpProxyClientResponse) {
        self.ctx.apply_proxy_status_ident(rsp);
    }

    fn rsp_hdr_recv_timeout(&self) -> Duration {
        self.site_ctx
            .rsp_hdr_recv_timeout()
            .unwrap_or(self.ctx.server_config.timeout.recv_rsp_header)
    }

    async fn run_connected<CDR, CDW>(
        &mut self,
        clt_r: HttpClientReader<CDR>,
        mut clt_w: HttpClientWriter<CDW>,
        (ups_r, ups_w): TcpConnection,
        rsp: HttpForwardRemoteResponse,
    ) -> ServerTaskResult<()>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        if self.ctx.server_config.flush_task_log_on_connected
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_connected();
        }

        self.task_notes.stage = ServerTaskStage::Replying;
        self.send_response_header(&mut clt_w, &rsp).await?;
        self.send_error_response = false;

        self.task_notes.mark_relaying();
        self.task_notes
            .foreach_req_stats(|s| s.req_ready.add_websocket());

        let clt_r = self.attach_websocket_relay_io(clt_r, &mut clt_w);

        match self.ups_r_leftover.take() {
            None => self.transit_transparent(clt_r, clt_w, ups_r, ups_w).await,
            Some(leftover) => {
                self.transit_transparent(
                    clt_r,
                    clt_w,
                    OnceBufReader::with_bytes(ups_r, leftover),
                    ups_w,
                )
                .await
            }
        }
    }

    async fn do_connect<CDR, CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_r: &mut HttpClientReader<CDR>,
        clt_w: &mut HttpClientWriter<CDW>,
    ) -> ServerTaskResult<Option<(TcpConnection, HttpForwardRemoteResponse)>>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        if self.task_notes.check_layered_rate_limit().is_err() {
            self.reply_too_many_requests(clt_w).await;
            return Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::RateLimited,
            ));
        }

        if self.task_notes.acquire_site_request_semaphores().is_err() {
            self.reply_too_many_requests(clt_w).await;
            return Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::FullyLoaded,
            ));
        }

        let tenant = self.task_notes.tenant_ctx().cloned();
        let mut audit_task = false;
        let tcp_client_misc_opts = if let Some(tenant) = &tenant {
            match tenant.check_upstream(&self.upstream) {
                AclAction::Permit | AclAction::PermitAndLog => {}
                AclAction::Forbid | AclAction::ForbidAndLog => {
                    self.reply_forbidden(clt_w).await;
                    return Err(ServerTaskError::ForbiddenByRule(
                        ServerTaskForbiddenError::DestDenied,
                    ));
                }
            }
            if let Some(action) = tenant.check_http_user_agent(
                req.end_to_end_headers
                    .get_all(header::USER_AGENT)
                    .iter()
                    .map(|v| v.to_str()),
            ) {
                match action {
                    AclAction::Permit | AclAction::PermitAndLog => {}
                    AclAction::Forbid | AclAction::ForbidAndLog => {
                        self.reply_forbidden(clt_w).await;
                        return Err(ServerTaskError::ForbiddenByRule(
                            ServerTaskForbiddenError::UaBlocked,
                        ));
                    }
                }
            }
            if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
                audit_task = tenant
                    .user()
                    .audit()
                    .do_task_audit()
                    .unwrap_or_else(|| audit_handle.do_task_audit());
            }
            tenant
                .user_config()
                .tcp_client_misc_opts(&self.ctx.server_config.tcp_misc_opts)
        } else {
            if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
                audit_task = audit_handle.do_task_audit();
            }
            Cow::Borrowed(&self.ctx.server_config.tcp_misc_opts)
        };

        self.ctx
            .cc_info
            .tcp_sock_set_raw_opts(&tcp_client_misc_opts, true)
            .map_err(|_| {
                ServerTaskError::InternalServerError("failed to set client socket options")
            })?;

        self.setup_clt_limit_and_stats(req, Some(clt_r), clt_w);

        let ups_c = self.get_new_connection(req, clt_w).await?;
        if audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.clone()
            && let Some(reqmod) = audit_handle.icap_reqmod_client()
        {
            return self.run_icap(req, clt_w, ups_c, reqmod).await;
        }
        self.handshake_origin(req, clt_w, ups_c).await
    }

    fn clt_speed_limit(&self) -> Option<TcpSockSpeedLimitConfig> {
        let server = self.ctx.server_config.tcp_sock_speed_limit;
        let limit = self
            .site_ctx
            .tcp_sock_speed_limit()
            .shrink_as_smaller(&server);
        if limit.eq(&server) { None } else { Some(limit) }
    }

    fn setup_clt_limit_and_stats<CDR, CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_r: Option<&mut HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
    ) where
        CDR: AsyncRead + Unpin,
        CDW: AsyncWrite + Unpin,
    {
        let origin_header_size = req.origin_header_size() as u64;
        self.task_stats.clt.read.add_bytes(origin_header_size);

        let limit_config = self.clt_speed_limit();
        clt_w.retain_global_limiter_by_group(GlobalLimitGroup::Server);
        if let Some(br) = clt_r {
            if let Some(limit_config) = &limit_config {
                br.reset_local_limit(limit_config.shift_millis, limit_config.max_north);
                clt_w.reset_local_limit(limit_config.shift_millis, limit_config.max_south);
            }
            if let Some(user) = self.task_notes.tenant_user() {
                if let Some(limiter) = user.tcp_all_upload_speed_limit() {
                    limiter.try_consume(origin_header_size);
                    br.add_global_limiter(limiter.clone());
                }
                if let Some(limiter) = user.tcp_all_download_speed_limit() {
                    clt_w.add_global_limiter(limiter.clone());
                }
            }
        } else if let Some(limit_config) = &limit_config {
            clt_w.reset_local_limit(limit_config.shift_millis, limit_config.max_south);
        }
    }

    fn attach_websocket_relay_io<CDR, CDW>(
        &self,
        clt_r: HttpClientReader<CDR>,
        clt_w: &mut HttpClientWriter<CDW>,
    ) -> LimitedReader<CDR>
    where
        CDR: AsyncRead + Unpin,
        CDW: AsyncWrite + Unpin,
    {
        let mut wrapper_stats =
            WebSocketTaskCltWrapperStats::new(&self.ctx.server_stats, &self.task_stats);
        wrapper_stats.push_user_io_stats(self.task_notes.fetch_traffic_stats(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        ));
        let (clt_r_stats, clt_w_stats) = wrapper_stats.split();
        let limit = self
            .site_ctx
            .tcp_sock_speed_limit()
            .shrink_as_smaller(&self.ctx.server_config.tcp_sock_speed_limit);
        let mut clt_r = LimitedReader::local_limited(
            clt_r.into_inner(),
            limit.shift_millis,
            limit.max_north,
            clt_r_stats,
        );
        if let Some(user) = self.task_notes.tenant_user()
            && let Some(limiter) = user.tcp_all_upload_speed_limit()
        {
            clt_r.add_global_limiter(limiter.clone());
        }
        clt_w.reset_stats(clt_w_stats);
        clt_r
    }

    async fn get_new_connection<CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut HttpClientWriter<CDW>,
    ) -> ServerTaskResult<TcpConnection>
    where
        CDW: AsyncWrite + Unpin,
    {
        self.task_notes.stage = ServerTaskStage::Connecting;

        match self.make_new_connection(req).await {
            Ok(ups_c) => {
                self.site_ctx
                    .site()
                    .record_peer_connect_result(&self.upstream, true);
                self.task_notes.stage = ServerTaskStage::Connected;
                Ok(ups_c)
            }
            Err(e) => {
                self.site_ctx
                    .site()
                    .record_peer_connect_result(&self.upstream, false);
                self.reply_connect_err(&e, clt_w).await;
                Err(e.into())
            }
        }
    }

    async fn make_new_connection(
        &mut self,
        req: &HttpProxyClientRequest,
    ) -> Result<TcpConnection, TcpConnectError> {
        self.task_notes
            .site_upstream()
            .map_err(|_| TcpConnectError::InternalServerError("failed to select site upstream"))?;
        let mut audit_ctx = AuditContext::new(self.ctx.audit_handle.clone());
        let task_stats: ArcTcpConnectionTaskRemoteStats = self.task_stats.clone();
        if let Some(tls_client) = self.site_ctx.site().tls_client() {
            let task_conf = TlsConnectTaskConf {
                tcp: TcpConnectTaskConf {
                    upstream: &self.upstream,
                },
                tls_config: tls_client,
                tls_name: self.site_ctx.site().tls_name_or(
                    req.host
                        .as_ref()
                        .map(|addr| addr.host())
                        .unwrap_or_else(|| self.site_ctx.site().tls_name()),
                ),
                alpn_protocols: None,
            };
            self.ctx
                .escaper
                .tls_setup_connection(
                    &task_conf,
                    &mut self.egress_notes,
                    &self.task_notes,
                    task_stats,
                    &mut audit_ctx,
                )
                .await
        } else {
            let task_conf = TcpConnectTaskConf {
                upstream: &self.upstream,
            };
            self.ctx
                .escaper
                .tcp_setup_connection(
                    &task_conf,
                    &mut self.egress_notes,
                    &self.task_notes,
                    task_stats,
                    &mut audit_ctx,
                )
                .await
        }
    }

    async fn run_icap<CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut HttpClientWriter<CDW>,
        ups_c: TcpConnection,
        reqmod: &IcapReqmodClient,
    ) -> ServerTaskResult<Option<(TcpConnection, HttpForwardRemoteResponse)>>
    where
        CDW: AsyncWrite + Unpin,
    {
        match reqmod
            .h1_adapter(
                self.ctx.server_config.tcp_copy,
                self.ctx.server_config.h1.body_line_max_len,
                true,
                self.ctx.idle_checker(&self.task_notes),
            )
            .await
        {
            Ok(mut adapter) => {
                adapter.set_client_addr(self.ctx.client_addr());
                if let Some(name) = self.task_notes.raw_user_name() {
                    adapter.set_client_username(name.clone());
                }
                if let Some(name) = self.task_notes.tenant_user_name() {
                    adapter.set_tenant_username(name.clone());
                }
                let mut adaptation_state =
                    ReqmodAdaptationRunState::new(self.task_notes.task_created_instant());
                self.forward_with_adaptation(req, clt_w, ups_c, adapter, &mut adaptation_state)
                    .await
            }
            Err(e) => {
                if reqmod.bypass() {
                    self.handshake_origin(req, clt_w, ups_c).await
                } else {
                    Err(ServerTaskError::InternalAdapterError(e))
                }
            }
        }
    }

    async fn forward_with_adaptation<CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut HttpClientWriter<CDW>,
        ups_c: TcpConnection,
        icap_adapter: HttpRequestAdapter<ServerIdleChecker>,
        adaptation_state: &mut ReqmodAdaptationRunState,
    ) -> ServerTaskResult<Option<(TcpConnection, HttpForwardRemoteResponse)>>
    where
        CDW: AsyncWrite + Unpin,
    {
        match icap_adapter.xfer_connect(adaptation_state, req).await {
            Ok(ReqmodAdaptationMidState::OriginalRequest) => {
                self.handshake_origin(req, clt_w, ups_c).await
            }
            Ok(ReqmodAdaptationMidState::AdaptedRequest(final_req)) => {
                self.handshake_origin(&final_req, clt_w, ups_c).await
            }
            Ok(ReqmodAdaptationMidState::HttpErrResponse(rsp, rsp_body)) => {
                self.send_adaptation_error_response(clt_w, rsp, rsp_body)
                    .await?;
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn send_adaptation_error_response<W>(
        &mut self,
        clt_w: &mut W,
        mut rsp: HttpAdapterErrorResponse,
        rsp_recv_body: Option<ReqmodRecvHttpResponseBody>,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        self.ctx
            .set_custom_header_for_adaptation_error_reply(&self.egress_notes, &mut rsp);

        let buf = rsp.serialize(true);
        self.send_error_response = false;
        clt_w
            .write_all(buf.as_ref())
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        self.ws_notes.rsp_status = rsp.status.as_u16();

        if let Some(mut recv_body) = rsp_recv_body {
            let mut body_reader = recv_body.body_reader();
            let mut copy_to_clt =
                StreamCopy::new(&mut body_reader, clt_w, &self.ctx.server_config.tcp_copy);
            (&mut copy_to_clt).await.map_err(|e| match e {
                StreamCopyError::ReadFailed(e) => ServerTaskError::InternalAdapterError(anyhow!(
                    "read http error response from adapter failed: {e:?}"
                )),
                StreamCopyError::WriteFailed(e) => ServerTaskError::ClientTcpWriteFailed(e),
            })?;
            recv_body.save_connection().await;
        } else {
            clt_w
                .flush()
                .await
                .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        }
        Ok(())
    }

    async fn handshake_origin<CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut HttpClientWriter<CDW>,
        (ups_r, mut ups_w): TcpConnection,
    ) -> ServerTaskResult<Option<(TcpConnection, HttpForwardRemoteResponse)>>
    where
        CDW: AsyncWrite + Unpin,
    {
        send_req_header_to_origin(&mut ups_w, req, None)
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        ups_w
            .flush()
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;

        let mut ups_r = FlexBufReader::new(ups_r);
        let rsp_header = match tokio::time::timeout(
            self.rsp_hdr_recv_timeout(),
            HttpForwardRemoteResponse::parse(
                &mut ups_r,
                &req.method,
                false,
                self.ctx.server_config.rsp_hdr_max_size,
            ),
        )
        .await
        {
            Ok(Ok(rsp_header)) => rsp_header,
            Ok(Err(e)) => return Err(ServerTaskError::from(e)),
            Err(_) => {
                return Err(ServerTaskError::UpstreamAppTimeout(
                    "timeout to receive response header",
                ));
            }
        };
        self.handle_origin_response(req, clt_w, ups_r, ups_w, rsp_header)
            .await
    }

    async fn handle_origin_response<CDW>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_r: FlexBufReader<Box<dyn AsyncRead + Unpin + Send + Sync>>,
        mut ups_w: Box<dyn AsyncWrite + Unpin + Send + Sync>,
        mut rsp_header: HttpForwardRemoteResponse,
    ) -> ServerTaskResult<Option<(TcpConnection, HttpForwardRemoteResponse)>>
    where
        CDW: AsyncWrite + Unpin,
    {
        rsp_header.set_no_keep_alive();
        self.ws_notes.rsp_status = rsp_header.code;
        self.ws_notes.origin_status = rsp_header.code;
        match rsp_header.code {
            101 => {
                let (leftover, ups_r) = ups_r.into_parts();
                self.ups_r_leftover = (!leftover.is_empty()).then_some(leftover);
                Ok(Some(((ups_r, ups_w), rsp_header)))
            }
            100..=199 => Err(ServerTaskError::InvalidUpstreamProtocol(
                "unexpected informational response",
            )),
            _ => {
                self.send_response_without_adaptation(req, clt_w, &mut ups_r, &rsp_header)
                    .await?;
                let _ = ups_w.shutdown().await;
                Ok(None)
            }
        }
    }

    async fn send_response_header<W>(
        &mut self,
        clt_w: &mut W,
        rsp: &HttpForwardRemoteResponse,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let buf = rsp.serialize();
        clt_w
            .write_all_flush(buf.as_ref())
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)
    }

    async fn send_response_without_adaptation<R, W>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut W,
        ups_r: &mut R,
        rsp_header: &HttpForwardRemoteResponse,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        self.send_error_response = false;
        if let Some(body_type) = rsp_header.body_type(&req.method) {
            let mut buf = Vec::with_capacity(self.ctx.server_config.tcp_copy.buffer_size());
            rsp_header.serialize_to(&mut buf);
            self.ws_notes.rsp_status = rsp_header.code;
            self.send_response_body(buf, clt_w, ups_r, body_type).await
        } else {
            self.send_response_header(clt_w, rsp_header).await?;
            self.ws_notes.rsp_status = rsp_header.code;
            Ok(())
        }
    }

    async fn send_response_body<R, W>(
        &mut self,
        header: Vec<u8>,
        clt_w: &mut W,
        ups_r: &mut R,
        body_type: HttpBodyType,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let header_len = header.len() as u64;
        let mut body_reader = HttpBodyReader::new(
            ups_r,
            body_type,
            self.ctx.server_config.h1.body_line_max_len,
        );
        let mut ups_to_clt = StreamCopy::with_data(
            &mut body_reader,
            clt_w,
            &self.ctx.server_config.tcp_copy,
            header,
        );
        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;
                r = &mut ups_to_clt => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            if matches!(&e, StreamCopyError::ReadFailed(_))
                                && ups_to_clt.copied_size() < header_len
                            {
                                let _ = ups_to_clt.write_flush().await;
                            }
                            Err(match e {
                                StreamCopyError::ReadFailed(e) => {
                                    ServerTaskError::UpstreamReadFailed(e)
                                }
                                StreamCopyError::WriteFailed(e) => {
                                    ServerTaskError::ClientTcpWriteFailed(e)
                                }
                            })
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if ups_to_clt.is_idle() {
                        idle_count += n;
                        if idle_count >= self.max_idle_count {
                            return if ups_to_clt.no_cached_data() {
                                Err(ServerTaskError::UpstreamAppTimeout(
                                    "idle while reading response body",
                                ))
                            } else {
                                Err(ServerTaskError::ClientAppTimeout(
                                    "idle while sending response body",
                                ))
                            };
                        }
                    } else {
                        idle_count = 0;
                        ups_to_clt.reset_active();
                    }
                    if self.ctx.server_quit_policy.force_quit() {
                        return Err(ServerTaskError::CanceledAsServerQuit);
                    }
                }
            }
        }
    }

    async fn reply_too_many_requests<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::too_many_requests(self.ws_notes.version);
        self.enable_custom_header_for_local_reply(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_ok() {
            self.ws_notes.rsp_status = rsp.status();
        }
    }

    async fn reply_forbidden<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::forbidden(self.ws_notes.version);
        self.enable_custom_header_for_local_reply(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_ok() {
            self.ws_notes.rsp_status = rsp.status();
        }
    }

    async fn reply_connect_err<W>(&mut self, e: &TcpConnectError, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp =
            HttpProxyClientResponse::from_tcp_connect_error(e, self.ws_notes.version, true);
        self.enable_custom_header_for_local_reply(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_ok() {
            self.ws_notes.rsp_status = rsp.status();
        }
    }

    async fn reply_task_err<W>(
        &mut self,
        _req: &HttpProxyClientRequest,
        e: &ServerTaskError,
        clt_w: &mut W,
    ) where
        W: AsyncWrite + Unpin,
    {
        let Some(mut rsp) = HttpProxyClientResponse::from_task_err(e, self.ws_notes.version, true)
        else {
            return;
        };
        self.enable_custom_header_for_local_reply(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_ok() {
            self.ws_notes.rsp_status = rsp.status();
        }
    }
}

impl StreamTransitTask for HttpGuardWebsocketTask {
    fn copy_config(&self) -> StreamCopyConfig {
        self.ctx.server_config.tcp_copy
    }

    fn idle_check_interval(&self) -> IdleInterval {
        self.ctx.idle_wheel.register()
    }

    fn max_idle_count(&self) -> usize {
        self.max_idle_count
    }

    fn log_client_shutdown(&self) {
        if let Some(log_ctx) = self.get_log_context() {
            log_ctx.log_client_shutdown();
        }
    }

    fn log_upstream_shutdown(&self) {
        if let Some(log_ctx) = self.get_log_context() {
            log_ctx.log_upstream_shutdown();
        }
    }

    fn log_periodic(&self) {
        if let Some(log_ctx) = self.get_log_context() {
            log_ctx.log_periodic();
        }
    }

    fn log_flush_interval(&self) -> Option<Duration> {
        self.ctx.log_flush_interval()
    }

    fn quit_policy(&self) -> &ServerQuitPolicy {
        self.ctx.server_quit_policy.as_ref()
    }

    fn user(&self) -> Option<&User> {
        None
    }
}
