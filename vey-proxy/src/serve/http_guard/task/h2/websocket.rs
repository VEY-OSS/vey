/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use bytes::Bytes;
use h2::client::SendRequest;
use h2::server::SendResponse;
use h2::{Reason, RecvStream, SendStream};
use http::{Request, Response, StatusCode, Version};
use tokio::time::Instant;

use vey_h2::H2BodyTransfer;
use vey_icap_client::reqmod::h2::{
    HttpAdapterErrorResponse, ReqmodAdaptationMidState, ReqmodAdaptationRunState,
    ReqmodRecvHttpResponseBody,
};
use vey_types::acl::AclAction;

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

pub(crate) struct H2WebsocketTask {
    ctx: Arc<CommonTaskContext>,
    site: Arc<Site>,
    task_notes: ServerTaskNotes,
    http_notes: HttpForwardTaskNotes,
    egress_notes: EgressNotes,
    task_stats: Arc<H2ForwardTaskStats>,
    send_error_response: bool,
    started: bool,
    _alive_guard: Option<H2ForwardTaskAliveGuard>,
    _site_req_alive_permits: SiteRequestPermits,
}

impl Drop for H2WebsocketTask {
    fn drop(&mut self) {
        if self.started {
            self._site_req_alive_permits.release();
            self.started = false;
        }
    }
}

impl H2WebsocketTask {
    pub(crate) fn new(
        ctx: Arc<CommonTaskContext>,
        site_ctx: SiteContext,
        site: Arc<Site>,
        req: &Request<RecvStream>,
    ) -> Self {
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
        H2WebsocketTask {
            ctx,
            site,
            task_notes,
            http_notes,
            egress_notes: EgressNotes::default(),
            task_stats: Arc::new(H2ForwardTaskStats::default()),
            send_error_response: false,
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
                task_type: "H2Websocket",
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

    pub(crate) async fn run(
        mut self,
        clt_req: Request<RecvStream>,
        mut clt_send_rsp: SendResponse<Bytes>,
    ) {
        self._alive_guard = Some(self.ctx.server_stats.add_h2_forward_task());
        self.task_notes
            .hold_req_alive(RequestAliveKind::HttpForward {
                is_https: self.site.tls_client().is_some(),
            });
        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log) = self.log_ctx()
        {
            log.log_created();
        }
        self.started = true;

        match self.do_run(clt_req, &mut clt_send_rsp).await {
            Ok(()) => {
                if let Some(log) = self.log_ctx() {
                    log.log("finished");
                }
            }
            Err(e) => {
                if self.send_error_response
                    && let Some((status, error)) = e.status_and_error()
                    && let Some(rsp) =
                        h2_local_error_response(&self.ctx.server_config, status, error)
                {
                    let _ = clt_send_rsp.send_response(rsp, true);
                }
                if let Some(log) = self.log_ctx() {
                    log.log(&e.to_string());
                }
            }
        }
    }

    async fn do_run(
        &mut self,
        clt_req: Request<RecvStream>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        if self.task_notes.check_layered_rate_limit().is_err() {
            self.reply_denied(clt_send_rsp, StatusCode::TOO_MANY_REQUESTS);
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
                self.reply_denied(clt_send_rsp, StatusCode::TOO_MANY_REQUESTS);
                return Err(H2StreamTransferError::InternalServerError("fully loaded"));
            }
        }

        if let Some(tenant) = self.task_notes.site_ctx().and_then(|s| s.tenant()) {
            match tenant.check_upstream(self.site.upstream()) {
                AclAction::Permit | AclAction::PermitAndLog => {}
                AclAction::Forbid | AclAction::ForbidAndLog => {
                    self.reply_denied(clt_send_rsp, StatusCode::FORBIDDEN);
                    return Err(H2StreamTransferError::InternalServerError("dest denied"));
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
        let (parts, clt_r) = clt_req.into_parts();
        let ups_req = Request::from_parts(parts, ());

        if let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(reqmod) = audit_handle.icap_reqmod_client()
        {
            match reqmod
                .h2_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.server_config.h2.max_header_list_size as usize,
                    self.ctx.server_config.timeout.recv_rsp_header,
                    true,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state = ReqmodAdaptationRunState::new(Instant::now());
                    adapter.set_client_addr(self.task_notes.client_addr());
                    if let Some(username) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(username.clone());
                    }
                    match adapter.xfer_connect(&mut adaptation_state, ups_req).await {
                        Ok(ReqmodAdaptationMidState::OriginalRequest(req))
                        | Ok(ReqmodAdaptationMidState::AdaptedRequest(_, req)) => {
                            return self
                                .send_connect(ups_send_req, req, clt_r, clt_send_rsp)
                                .await;
                        }
                        Ok(ReqmodAdaptationMidState::HttpErrResponse(err_rsp, recv_body)) => {
                            return self
                                .send_adaptation_error_response(clt_send_rsp, err_rsp, recv_body)
                                .await;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(e) => {
                    if !reqmod.bypass() {
                        return Err(H2StreamTransferError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.send_connect(ups_send_req, ups_req, clt_r, clt_send_rsp)
            .await
    }

    fn reply_denied(&mut self, clt_send_rsp: &mut SendResponse<Bytes>, status: StatusCode) {
        if let Some(rsp) = h2_local_error_response(
            &self.ctx.server_config,
            status,
            ProxyErrorType::HttpRequestDenied,
        ) {
            let _ = clt_send_rsp.send_response(rsp, true);
            self.http_notes.rsp_status = status.as_u16();
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
        self.http_notes.rsp_status = response.status().as_u16();
        if let Some(mut recv_body) = rsp_recv_body {
            let mut clt_send_stream = clt_send_rsp
                .send_response(response, false)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            recv_body
                .body_transfer(&mut clt_send_stream)
                .await
                .map_err(|e| {
                    H2StreamTransferError::InternalAdapterError(anyhow::anyhow!("{e:?}"))
                })?;
            recv_body.save_connection().await;
        } else {
            clt_send_rsp
                .send_response(response, true)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
        }
        Ok(())
    }

    async fn send_connect(
        &mut self,
        mut ups_send_req: SendRequest<Bytes>,
        ups_req: Request<()>,
        clt_r: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        let (ups_response_fut, ups_w) = ups_send_req
            .send_request(ups_req, false)
            .map_err(H2StreamTransferError::RequestHeadSendFailed)?;
        self.http_notes.mark_req_send_hdr();

        let ups_rsp = tokio::time::timeout(
            self.ctx.server_config.timeout.recv_rsp_header,
            ups_response_fut,
        )
        .await
        .map_err(|_| H2StreamTransferError::ResponseHeadRecvTimeout)?
        .map_err(H2StreamTransferError::ResponseHeadRecvFailed)?;
        self.http_notes.mark_rsp_recv_hdr();
        self.http_notes.origin_status = ups_rsp.status().as_u16();
        self.send_error_response = false;

        if !ups_rsp.status().is_success() {
            let (parts, body) = ups_rsp.into_parts();
            let rsp = Response::from_parts(parts, ());
            if body.is_end_stream() {
                clt_send_rsp
                    .send_response(rsp, true)
                    .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            } else {
                let clt_w = clt_send_rsp
                    .send_response(rsp, false)
                    .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
                H2BodyTransfer::new(body, clt_w, self.ctx.server_config.tcp_copy.yield_size())
                    .await
                    .map_err(H2StreamTransferError::ResponseBodyTransferFailed)?;
            }
            self.http_notes.rsp_status = self.http_notes.origin_status;
            return Ok(());
        }

        let (parts, ups_r) = ups_rsp.into_parts();
        let rsp = Response::from_parts(parts, ());
        if ups_r.is_end_stream() {
            clt_send_rsp
                .send_response(rsp, true)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            self.http_notes.rsp_status = self.http_notes.origin_status;
            return Ok(());
        }
        let clt_w = clt_send_rsp
            .send_response(rsp, false)
            .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
        self.http_notes.rsp_status = self.http_notes.origin_status;
        copy_ws_streams(
            clt_r,
            clt_w,
            ups_r,
            ups_w,
            self.ctx.server_config.tcp_copy.yield_size(),
            &self.ctx,
            &self.task_notes,
        )
        .await
    }
}

async fn copy_ws_streams(
    clt_r: RecvStream,
    clt_w: SendStream<Bytes>,
    ups_r: RecvStream,
    ups_w: SendStream<Bytes>,
    yield_size: usize,
    ctx: &CommonTaskContext,
    task_notes: &ServerTaskNotes,
) -> Result<(), H2StreamTransferError> {
    let mut c2u = H2BodyTransfer::new(clt_r, ups_w, yield_size);
    let mut u2c = H2BodyTransfer::new(ups_r, clt_w, yield_size);
    let mut idle_interval = ctx.idle_wheel.register();
    let mut idle_count = 0;
    let mut c2u_done = false;
    let mut u2c_done = false;
    loop {
        tokio::select! {
            biased;
            r = &mut c2u, if !c2u_done => {
                r.map_err(H2StreamTransferError::RequestBodyTransferFailed)?;
                c2u_done = true;
                if u2c_done {
                    return Ok(());
                }
            }
            r = &mut u2c, if !u2c_done => {
                r.map_err(H2StreamTransferError::ResponseBodyTransferFailed)?;
                u2c_done = true;
                if c2u_done {
                    return Ok(());
                }
            }
            n = idle_interval.tick() => {
                let idle = (c2u_done || c2u.is_idle()) && (u2c_done || u2c.is_idle());
                if idle {
                    idle_count += n;
                    if idle_count > task_notes.task_max_idle_count(ctx.server_config.task_idle_max_count) {
                        return Err(H2StreamTransferError::Idle(idle_interval.period(), idle_count));
                    }
                } else {
                    idle_count = 0;
                    c2u.reset_active();
                    u2c.reset_active();
                }
                if ctx.server_quit_policy.force_quit() {
                    return Err(H2StreamTransferError::CanceledAsServerQuit);
                }
                if task_notes
                    .site_ctx()
                    .and_then(|s| s.tenant())
                    .is_some_and(|t| t.user().is_blocked())
                {
                    return Err(H2StreamTransferError::CanceledAsUserBlocked);
                }
            }
        }
    }
}
