/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use bytes::Bytes;
use h2::client::SendRequest;
use h2::server::SendResponse;
use h2::{Reason, RecvStream};
use http::{Request, Response, StatusCode, Version, header};
use tokio::time::Instant;

use vey_h2::{H2BodyTransfer, H2ResponseHeaderReceiver, RequestExt};
use vey_icap_client::reqmod::h2::{
    H2RequestAdapter, HttpAdapterErrorResponse, ReqmodAdaptationEndState, ReqmodAdaptationRunState,
    ReqmodRecvHttpResponseBody,
};
use vey_icap_client::respmod::h2::{RespmodAdaptationEndState, RespmodAdaptationRunState};
use vey_types::acl::AclAction;
use vey_types::net::HttpHeaderMap;

use super::CommonTaskContext;
use super::error::{H2StreamTransferError, h2_local_error_response};
use super::origin;
use super::stats::H2ForwardTaskStats;
use crate::escape::EgressNotes;
use crate::log::task::h2_forward::TaskLogForH2Forward;
use crate::module::http_forward::HttpForwardTaskNotes;
use crate::module::http_header::ProxyErrorType;
use crate::serve::http_guard::H2ForwardTaskAliveGuard;
use crate::serve::{ServerTaskNotes, ServerTaskStage};
use crate::site::{Site, SiteContext, SiteRequestPermits};
use crate::stat::types::RequestAliveKind;

pub(crate) struct H2ForwardTask {
    ctx: Arc<CommonTaskContext>,
    site: Arc<Site>,
    task_notes: ServerTaskNotes,
    http_notes: HttpForwardTaskNotes,
    egress_notes: EgressNotes,
    task_stats: Arc<H2ForwardTaskStats>,
    send_error_response: bool,
    allow_continue: bool,
    is_https: bool,
    started: bool,
    _alive_guard: Option<H2ForwardTaskAliveGuard>,
    _site_req_alive_permits: SiteRequestPermits,
}

impl Drop for H2ForwardTask {
    fn drop(&mut self) {
        if self.started {
            self._site_req_alive_permits.release();
            self.started = false;
        }
    }
}

impl H2ForwardTask {
    pub(crate) fn new(
        ctx: Arc<CommonTaskContext>,
        site_ctx: SiteContext,
        site: Arc<Site>,
        req: &Request<RecvStream>,
    ) -> Self {
        let is_https = site.tls_client().is_some();
        let uri_log_max_chars = site_ctx
            .tenant()
            .and_then(|c| c.user_config().log_uri_max_chars)
            .unwrap_or(ctx.server_config.log_uri_max_chars);
        let now = Instant::now();
        let http_notes = HttpForwardTaskNotes::new(
            now,
            now,
            req.method().clone(),
            req.uri().clone(),
            uri_log_max_chars,
        );
        let task_notes = ServerTaskNotes::new(ctx.cc_info.clone(), None, Default::default())
            .with_site_ctx(site_ctx);
        let allow_continue = req.expect_100_continue();
        H2ForwardTask {
            ctx,
            site,
            task_notes,
            http_notes,
            egress_notes: EgressNotes::default(),
            task_stats: Arc::new(H2ForwardTaskStats::default()),
            send_error_response: false,
            allow_continue,
            is_https,
            started: false,
            _alive_guard: None,
            _site_req_alive_permits: SiteRequestPermits::default(),
        }
    }

    fn log_ctx(&self) -> Option<TaskLogForH2Forward<'_>> {
        self.ctx
            .task_logger
            .as_ref()
            .map(|logger| TaskLogForH2Forward {
                logger,
                task_type: "H2Forward",
                upstream: self.site.upstream(),
                task_notes: &self.task_notes,
                http_notes: &self.http_notes,
                egress_notes: &self.egress_notes,
                client_rd_bytes: self.task_stats.clt.read.get_bytes(),
                client_wr_bytes: self.task_stats.clt.write.get_bytes(),
                remote_rd_bytes: self.task_stats.ups.read.get_bytes(),
                remote_wr_bytes: self.task_stats.ups.write.get_bytes(),
            })
    }

    pub(crate) async fn forward(
        mut self,
        clt_req: Request<RecvStream>,
        mut clt_send_rsp: SendResponse<Bytes>,
    ) {
        self.pre_start();
        if let Err(e) = self.do_forward(clt_req, &mut clt_send_rsp).await {
            if self.send_error_response {
                self.reply_err(&mut clt_send_rsp, &e);
            }
            if let Some(log) = self.log_ctx() {
                log.log(&e.to_string());
            }
        } else if let Some(log) = self.log_ctx() {
            log.log("finished");
        }
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_h2_forward_task());
        self.task_notes
            .hold_req_alive(RequestAliveKind::HttpForward {
                is_https: self.is_https,
            });
        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log) = self.log_ctx()
        {
            log.log_created();
        }
        self.started = true;
    }

    fn reply_err(&mut self, clt_send_rsp: &mut SendResponse<Bytes>, e: &H2StreamTransferError) {
        if let Some((status, error)) = e.status_and_error()
            && let Some(rsp) = h2_local_error_response(&self.ctx.server_config, status, error)
        {
            let rsp_status = rsp.status().as_u16();
            if clt_send_rsp.send_response(rsp, true).is_ok() {
                self.http_notes.rsp_status = rsp_status;
            }
        }
    }

    fn tenant_blocked(&self) -> bool {
        self.task_notes
            .site_ctx()
            .and_then(|s| s.tenant())
            .is_some_and(|t| t.user().is_blocked())
    }

    async fn do_forward(
        &mut self,
        clt_req: Request<RecvStream>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        if self.task_notes.check_layered_rate_limit().is_err() {
            if let Some(rsp) = h2_local_error_response(
                &self.ctx.server_config,
                StatusCode::TOO_MANY_REQUESTS,
                ProxyErrorType::HttpRequestDenied,
            ) {
                let _ = clt_send_rsp.send_response(rsp, true);
                self.http_notes.rsp_status = StatusCode::TOO_MANY_REQUESTS.as_u16();
            }
            return Err(H2StreamTransferError::InternalServerError("rate limited"));
        }
        match self
            .task_notes
            .site_ctx()
            .expect("site context")
            .acquire_request_semaphores()
        {
            Ok(permits) => self._site_req_alive_permits = permits,
            Err(_) => {
                if let Some(rsp) = h2_local_error_response(
                    &self.ctx.server_config,
                    StatusCode::TOO_MANY_REQUESTS,
                    ProxyErrorType::HttpRequestDenied,
                ) {
                    let _ = clt_send_rsp.send_response(rsp, true);
                    self.http_notes.rsp_status = StatusCode::TOO_MANY_REQUESTS.as_u16();
                }
                return Err(H2StreamTransferError::InternalServerError("fully loaded"));
            }
        }

        if let Some(tenant) = self.task_notes.site_ctx().and_then(|s| s.tenant()) {
            match tenant.check_upstream(self.site.upstream()) {
                AclAction::Permit | AclAction::PermitAndLog => {}
                AclAction::Forbid | AclAction::ForbidAndLog => {
                    if let Some(rsp) = h2_local_error_response(
                        &self.ctx.server_config,
                        StatusCode::FORBIDDEN,
                        ProxyErrorType::HttpRequestDenied,
                    ) {
                        let _ = clt_send_rsp.send_response(rsp, true);
                        self.http_notes.rsp_status = StatusCode::FORBIDDEN.as_u16();
                    }
                    return Err(H2StreamTransferError::InternalServerError("dest denied"));
                }
            }
            if let Some(ua) = clt_req.headers().get(header::USER_AGENT) {
                let mut map = HttpHeaderMap::default();
                map.append(header::USER_AGENT, unsafe {
                    vey_types::net::HttpHeaderValue::from_buf_unchecked(ua.as_bytes().to_vec())
                });
                if let Some(action) = tenant.check_http_user_agent(&map)
                    && matches!(action, AclAction::Forbid | AclAction::ForbidAndLog)
                {
                    if let Some(rsp) = h2_local_error_response(
                        &self.ctx.server_config,
                        StatusCode::FORBIDDEN,
                        ProxyErrorType::HttpRequestDenied,
                    ) {
                        let _ = clt_send_rsp.send_response(rsp, true);
                        self.http_notes.rsp_status = StatusCode::FORBIDDEN.as_u16();
                    }
                    return Err(H2StreamTransferError::InternalServerError("ua denied"));
                }
            }
        }

        let origin = origin::checkout_or_connect(&self.ctx, &self.site, &self.task_notes).await?;
        self.http_notes.reused_connection = origin.reused;
        self.egress_notes = origin.egress_notes;
        self.task_notes.stage = ServerTaskStage::Connected;

        let ups_send_req = match tokio::time::timeout(
            self.ctx.server_config.h2.upstream_stream_open_timeout,
            origin.sender.ready(),
        )
        .await
        {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                let reason = e.reason().unwrap_or(Reason::REFUSED_STREAM);
                clt_send_rsp.send_reset(reason);
                return Err(H2StreamTransferError::UpstreamStreamOpenFailed(e));
            }
            Err(_) => {
                clt_send_rsp.send_reset(Reason::REFUSED_STREAM);
                return Err(H2StreamTransferError::UpstreamStreamOpenTimeout);
            }
        };

        self.send_error_response = true;
        let (parts, clt_body) = clt_req.into_parts();
        let ups_req = Request::from_parts(parts, ());

        let audit_task = self
            .task_notes
            .site_ctx()
            .and_then(|s| s.tenant())
            .and_then(|t| t.user_config().audit.do_task_audit())
            .unwrap_or_else(|| {
                self.ctx
                    .audit_handle
                    .as_ref()
                    .is_some_and(|h| h.do_task_audit())
            });

        if audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(reqmod) = audit_handle.icap_reqmod_client()
        {
            match reqmod
                .h2_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.server_config.h2.max_header_list_size as usize,
                    self.rsp_hdr_timeout(),
                    true,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state = ReqmodAdaptationRunState::new(Instant::now());
                    adapter.set_client_addr(self.task_notes.client_addr());
                    if let Some(username) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(username.clone());
                    }
                    if let Some(username) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(username.clone());
                    }
                    return self
                        .forward_with_adaptation(
                            ups_send_req,
                            ups_req,
                            clt_body,
                            clt_send_rsp,
                            adapter,
                            &mut adaptation_state,
                        )
                        .await;
                }
                Err(e) => {
                    if !reqmod.bypass() {
                        return Err(H2StreamTransferError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.forward_without_adaptation(ups_send_req, ups_req, clt_body, clt_send_rsp)
            .await
    }

    fn rsp_hdr_timeout(&self) -> std::time::Duration {
        self.task_notes
            .site_ctx()
            .and_then(|s| s.rsp_hdr_recv_timeout())
            .unwrap_or(self.ctx.server_config.timeout.recv_rsp_header)
    }

    async fn forward_with_adaptation(
        &mut self,
        ups_send_req: SendRequest<Bytes>,
        ups_req: Request<()>,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
        icap_adapter: H2RequestAdapter<crate::serve::ServerIdleChecker>,
        adaptation_state: &mut ReqmodAdaptationRunState,
    ) -> Result<(), H2StreamTransferError> {
        let orig_req = ups_req.clone_header();
        match icap_adapter
            .xfer(
                adaptation_state,
                ups_req,
                clt_body,
                ups_send_req,
                clt_send_rsp,
            )
            .await
        {
            Ok(ReqmodAdaptationEndState::OriginalTransferred(ups_rsp))
            | Ok(ReqmodAdaptationEndState::AdaptedTransferred(_, ups_rsp)) => {
                self.send_response(
                    orig_req,
                    ups_rsp,
                    clt_send_rsp,
                    adaptation_state.take_respond_shared_headers(),
                )
                .await
            }
            Ok(ReqmodAdaptationEndState::HttpErrResponse(err_rsp, recv_body)) => {
                self.send_adaptation_error_response(clt_send_rsp, err_rsp, recv_body)
                    .await
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn send_adaptation_error_response(
        &mut self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        rsp: HttpAdapterErrorResponse,
        rsp_recv_body: Option<ReqmodRecvHttpResponseBody>,
    ) -> Result<(), H2StreamTransferError> {
        let mut parts = Response::new(()).into_parts().0;
        parts.version = Version::HTTP_2;
        parts.status = rsp.status;
        parts.headers = rsp.headers.into();
        let response = Response::from_parts(parts, ());
        self.send_error_response = false;
        let rsp_status = response.status().as_u16();
        if let Some(mut recv_body) = rsp_recv_body {
            let mut clt_send_stream = clt_send_rsp
                .send_response(response, false)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            self.http_notes.rsp_status = rsp_status;
            recv_body
                .body_transfer(&mut clt_send_stream)
                .await
                .map_err(|e| {
                    H2StreamTransferError::InternalAdapterError(anyhow::anyhow!(
                        "adapter error body: {e:?}"
                    ))
                })?;
            recv_body.save_connection().await;
        } else {
            clt_send_rsp
                .send_response(response, true)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            self.http_notes.rsp_status = rsp_status;
        }
        Ok(())
    }

    async fn forward_without_adaptation(
        &mut self,
        mut ups_send_req: SendRequest<Bytes>,
        ups_req: Request<()>,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        let orig_req = ups_req.clone_header();
        let end_stream = clt_body.is_end_stream();
        let (ups_rsp_fut, ups_send_stream) = ups_send_req
            .send_request(ups_req, end_stream)
            .map_err(H2StreamTransferError::RequestHeadSendFailed)?;
        self.http_notes.mark_req_send_hdr();
        if end_stream {
            self.http_notes.mark_req_no_body();
        }

        if !end_stream {
            let mut req_body_transfer = H2BodyTransfer::new(
                clt_body,
                ups_send_stream,
                self.ctx.server_config.tcp_copy.yield_size(),
            );
            let mut idle_interval = self.ctx.idle_wheel.register();
            let mut idle_count = 0;
            loop {
                tokio::select! {
                    biased;
                    r = &mut req_body_transfer => {
                        r.map_err(H2StreamTransferError::RequestBodyTransferFailed)?;
                        self.http_notes.mark_req_send_all();
                        break;
                    }
                    n = idle_interval.tick() => {
                        if req_body_transfer.is_idle() {
                            idle_count += n;
                            if idle_count > self.task_notes.task_max_idle_count(self.ctx.server_config.task_idle_max_count) {
                                return Err(H2StreamTransferError::Idle(idle_interval.period(), idle_count));
                            }
                        } else {
                            idle_count = 0;
                            req_body_transfer.reset_active();
                        }
                        if self.tenant_blocked() {
                            return Err(H2StreamTransferError::CanceledAsUserBlocked);
                        }
                        if self.ctx.server_quit_policy.force_quit() {
                            return Err(H2StreamTransferError::CanceledAsServerQuit);
                        }
                    }
                }
            }
        }

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(ups_rsp_fut);
        let ups_rsp = tokio::time::timeout(self.rsp_hdr_timeout(), async {
            loop {
                let rsp = ups_recv_rsp
                    .recv_header()
                    .await
                    .map_err(H2StreamTransferError::ResponseHeadRecvFailed)?;
                match rsp.status() {
                    StatusCode::CONTINUE => {
                        if self.allow_continue {
                            clt_send_rsp
                                .send_informational(rsp)
                                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
                            self.allow_continue = false;
                        } else {
                            return Err(H2StreamTransferError::InvalidContinueResponse);
                        }
                    }
                    StatusCode::EARLY_HINTS => {
                        clt_send_rsp
                            .send_informational(rsp)
                            .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
                    }
                    _ => {
                        self.http_notes.mark_rsp_recv_hdr();
                        let body = ups_recv_rsp.take_body().ok_or(
                            H2StreamTransferError::UnsupportedInformationalResponse(rsp.status()),
                        )?;
                        let (headers, _) = rsp.into_parts();
                        return Ok(Response::from_parts(headers, body));
                    }
                }
            }
        })
        .await
        .map_err(|_| H2StreamTransferError::ResponseHeadRecvTimeout)??;

        self.send_response(orig_req, ups_rsp, clt_send_rsp, None)
            .await
    }

    async fn send_response(
        &mut self,
        ups_req: Request<()>,
        ups_rsp: Response<RecvStream>,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adaptation_respond_shared_headers: Option<HttpHeaderMap>,
    ) -> Result<(), H2StreamTransferError> {
        let (parts, ups_body) = ups_rsp.into_parts();
        let clt_rsp = Response::from_parts(parts, ());
        self.http_notes.origin_status = clt_rsp.status().as_u16();

        let audit_task = self
            .task_notes
            .site_ctx()
            .and_then(|s| s.tenant())
            .and_then(|t| t.user_config().audit.do_task_audit())
            .unwrap_or_else(|| {
                self.ctx
                    .audit_handle
                    .as_ref()
                    .is_some_and(|h| h.do_task_audit())
            });

        if audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(respmod) = audit_handle.icap_respmod_client()
        {
            match respmod
                .h2_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.server_config.h2.max_header_list_size as usize,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state = RespmodAdaptationRunState::new(
                        Instant::now(),
                        self.http_notes.dur_rsp_recv_hdr,
                    );
                    adapter.set_client_addr(self.task_notes.client_addr());
                    if let Some(username) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(username);
                    }
                    if let Some(username) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(username);
                    }
                    adapter.set_respond_shared_headers(adaptation_respond_shared_headers);
                    match adapter
                        .xfer(
                            &mut adaptation_state,
                            &ups_req,
                            clt_rsp,
                            ups_body,
                            clt_send_rsp,
                        )
                        .await
                    {
                        Ok(RespmodAdaptationEndState::OriginalTransferred)
                        | Ok(RespmodAdaptationEndState::AdaptedTransferred(_)) => {
                            self.http_notes.rsp_status = self.http_notes.origin_status;
                            return Ok(());
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(e) => {
                    if !respmod.bypass() {
                        return Err(H2StreamTransferError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.send_error_response = false;
        if ups_body.is_end_stream() {
            self.http_notes.mark_rsp_no_body();
            clt_send_rsp
                .send_response(clt_rsp, true)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            self.http_notes.rsp_status = self.http_notes.origin_status;
            return Ok(());
        }

        let clt_send_stream = clt_send_rsp
            .send_response(clt_rsp, false)
            .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
        self.http_notes.rsp_status = self.http_notes.origin_status;
        let mut rsp_body_transfer = H2BodyTransfer::new(
            ups_body,
            clt_send_stream,
            self.ctx.server_config.tcp_copy.yield_size(),
        );
        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;
                r = &mut rsp_body_transfer => {
                    r.map_err(H2StreamTransferError::ResponseBodyTransferFailed)?;
                    self.http_notes.mark_rsp_recv_all();
                    return Ok(());
                }
                n = idle_interval.tick() => {
                    if rsp_body_transfer.is_idle() {
                        idle_count += n;
                        if idle_count > self.task_notes.task_max_idle_count(self.ctx.server_config.task_idle_max_count) {
                            return Err(H2StreamTransferError::Idle(idle_interval.period(), idle_count));
                        }
                    } else {
                        idle_count = 0;
                        rsp_body_transfer.reset_active();
                    }
                    if self.tenant_blocked() {
                        return Err(H2StreamTransferError::CanceledAsUserBlocked);
                    }
                    if self.ctx.server_quit_policy.force_quit() {
                        return Err(H2StreamTransferError::CanceledAsServerQuit);
                    }
                }
            }
        }
    }
}
