/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use bytes::Bytes;
use h2::client::{ResponseFuture, SendRequest};
use h2::server::SendResponse;
use h2::{RecvStream, SendStream, StreamId};
use http::{HeaderMap, Request, Response, StatusCode, Version};
use tokio::time::Instant;

use vey_h2::{H2BodyTransfer, H2ResponseHeaderReceiver, RequestExt};
use vey_icap_client::reqmod::h2::{
    H2RequestAdapter, HttpAdapterErrorResponse, ReqmodAdaptationEndState, ReqmodAdaptationRunState,
    ReqmodRecvHttpResponseBody,
};
use vey_icap_client::respmod::h2::{RespmodAdaptationEndState, RespmodAdaptationRunState};
use vey_types::net::UpstreamAddr;

use super::{H2StreamTransferError, H2TaskContext, OriginConnection, OriginH2Sender};
use crate::escape::EgressNotes;
use crate::log::task::h2_forward::TaskLogForH2Forward;
use crate::module::http_forward::HttpForwardTaskNotes;
use crate::module::http_header::ProxyErrorType;
use crate::serve::http_guard::H2ForwardTaskAliveGuard;
use crate::serve::{ServerTaskNotes, ServerTaskStage};
use crate::stat::types::RequestAliveKind;

pub(crate) struct H2ForwardTask {
    pub(super) ctx: Arc<H2TaskContext>,
    pub(super) req: Request<()>,
    pub(super) clt_stream_id: StreamId,
    pub(super) ups_stream_id: Option<StreamId>,
    pub(super) task_notes: ServerTaskNotes,
    pub(super) http_notes: HttpForwardTaskNotes,
    pub(super) egress_notes: EgressNotes,
    pub(super) send_error_response: bool,
    pub(super) allow_continue: bool,
    pub(super) audit_task: bool,
    pub(super) upstream: UpstreamAddr,
    _alive_guard: Option<H2ForwardTaskAliveGuard>,
}

impl H2ForwardTask {
    pub(crate) fn new(ctx: Arc<H2TaskContext>, clt_stream_id: StreamId, req: Request<()>) -> Self {
        let uri_log_max_chars = ctx
            .site_ctx
            .log_uri_max_chars()
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
            .with_site_ctx(ctx.site_ctx_for_request());
        let allow_continue = req.expect_100_continue();
        H2ForwardTask {
            ctx,
            req,
            clt_stream_id,
            ups_stream_id: None,
            task_notes,
            http_notes,
            egress_notes: EgressNotes::default(),
            send_error_response: true,
            allow_continue,
            audit_task: false,
            upstream: UpstreamAddr::empty(),
            _alive_guard: None,
        }
    }

    pub(crate) fn task_id(&self) -> &uuid::Uuid {
        &self.task_notes.id
    }

    pub(super) fn log_ctx(&self) -> Option<TaskLogForH2Forward<'_>> {
        self.ctx
            .task_logger
            .as_ref()
            .map(|logger| TaskLogForH2Forward {
                logger,
                upstream: &self.upstream,
                task_notes: &self.task_notes,
                http_notes: &self.http_notes,
                egress_notes: &self.egress_notes,
                clt_stream_id: &self.clt_stream_id,
                ups_stream_id: self.ups_stream_id.as_ref(),
                connection_id: &self.ctx.connection_id,
            })
    }

    pub(crate) async fn forward(
        mut self,
        clt_body: RecvStream,
        mut clt_send_rsp: SendResponse<Bytes>,
    ) {
        self.pre_start();
        if let Err(e) = self.do_forward(clt_body, &mut clt_send_rsp).await {
            self.reply_task_err(&mut clt_send_rsp, &e);
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
            .hold_req_alive(RequestAliveKind::HttpForward { is_https: false });
        // TODO: site request traffic (http_forward) and task
        // byte stats. Needs h2 header/trailer frame sizes plus DATA on this stream.
        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log) = self.log_ctx()
        {
            log.log_created();
        }
    }

    fn reply_local_error(
        &mut self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        status: StatusCode,
        error: ProxyErrorType,
    ) {
        if let Some(rsp_status) = self.ctx.reply_local_error(clt_send_rsp, status, error) {
            self.http_notes.rsp_status = rsp_status;
            self.send_error_response = false;
        }
    }

    fn reply_task_err(
        &mut self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        e: &H2StreamTransferError,
    ) {
        if self.send_error_response
            && let Some((status, error)) = e.status_and_error()
        {
            self.reply_local_error(clt_send_rsp, status, error);
        }
    }

    fn reply_denied(&mut self, clt_send_rsp: &mut SendResponse<Bytes>, status: StatusCode) {
        if self.send_error_response {
            self.reply_local_error(clt_send_rsp, status, ProxyErrorType::HttpRequestDenied);
        }
    }

    async fn do_forward(
        &mut self,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        if let Some(site_ctx) = self.task_notes.site_ctx() {
            if site_ctx.check_rate_limit().is_err() {
                self.reply_denied(clt_send_rsp, StatusCode::TOO_MANY_REQUESTS);
                return Err(H2StreamTransferError::InternalServerError("rate limited"));
            }
            let site_ctx = site_ctx.clone();
            if self
                .task_notes
                .acquire_site_request_semaphores(&site_ctx)
                .is_err()
            {
                self.reply_denied(clt_send_rsp, StatusCode::TOO_MANY_REQUESTS);
                return Err(H2StreamTransferError::InternalServerError("fully loaded"));
            }
            if let Some(tenant) = site_ctx.tenant_ctx() {
                if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
                    self.audit_task = tenant
                        .user()
                        .audit()
                        .do_task_audit()
                        .unwrap_or_else(|| audit_handle.do_task_audit());
                }
            } else if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
                self.audit_task = audit_handle.do_task_audit();
            }
        } else if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
            self.audit_task = audit_handle.do_task_audit();
        }
        self.prepare_upstream()?;

        let request_host = self.req.host();
        let origin = self
            .ctx
            .checkout_or_connect(&mut self.task_notes, &self.upstream, &request_host)
            .await?;
        match origin {
            OriginConnection::H2(origin) => {
                self.forward_h2_origin(origin, clt_body, clt_send_rsp)
                    .await?;
            }
            OriginConnection::H1(origin) => {
                self.forward_h1_origin(origin, clt_body, clt_send_rsp)
                    .await?;
            }
        }
        self.task_notes.stage = ServerTaskStage::Finished;
        Ok(())
    }

    fn prepare_upstream(&mut self) -> Result<(), H2StreamTransferError> {
        self.upstream = self
            .ctx
            .site_ctx
            .site()
            .select_upstream(self.ctx.client_ip())
            .map_err(H2StreamTransferError::OriginConnectFailed)?;
        Ok(())
    }

    pub(super) fn mark_relaying(&mut self) {
        self.task_notes.mark_relaying();
        self.task_notes
            .foreach_req_stats(|s| s.req_ready.add_http_forward(false));
    }

    async fn forward_h2_origin(
        &mut self,
        origin: OriginH2Sender,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        if self.audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(reqmod) = audit_handle.icap_reqmod_client()
        {
            match reqmod
                .h2_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.server_config.h2.max_header_list_size as usize,
                    self.ctx.rsp_hdr_timeout(),
                    true,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state =
                        ReqmodAdaptationRunState::new(self.task_notes.task_created_instant());
                    adapter.set_client_addr(self.task_notes.client_addr());
                    if let Some(username) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(username.clone());
                    }
                    if let Some(username) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(username.clone());
                    }
                    // The adapter sends the request head itself.
                    let request_host = self.req.host();
                    let origin = match self
                        .ctx
                        .ready_h2_sender(
                            &mut self.task_notes,
                            &self.upstream,
                            &request_host,
                            origin,
                        )
                        .await
                    {
                        Ok(origin) => origin,
                        Err(e) => {
                            H2TaskContext::reset_unopened_stream(clt_send_rsp, &e);
                            return Err(e);
                        }
                    };
                    self.set_h2_origin_connected(origin.reused, origin.egress_notes);
                    return self
                        .forward_with_adaptation(
                            origin.sender,
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

        self.forward_without_adaptation(origin, clt_body, clt_send_rsp)
            .await
    }

    fn set_h2_origin_connected(&mut self, reused: bool, egress_notes: EgressNotes) {
        self.http_notes.reused_connection = reused;
        self.egress_notes = egress_notes;
        self.task_notes.stage = ServerTaskStage::Connected;
        self.mark_relaying();
    }

    async fn forward_with_adaptation(
        &mut self,
        ups_send_req: SendRequest<Bytes>,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
        icap_adapter: H2RequestAdapter<crate::serve::ServerIdleChecker>,
        adaptation_state: &mut ReqmodAdaptationRunState,
    ) -> Result<(), H2StreamTransferError> {
        let end_state = icap_adapter
            .xfer(
                adaptation_state,
                self.req.clone_header(),
                clt_body,
                ups_send_req,
                clt_send_rsp,
            )
            .await;
        if let Some(dur) = adaptation_state.dur_ups_send_header {
            self.http_notes.dur_req_send_hdr = dur;
        }
        if let Some(dur) = adaptation_state.dur_ups_send_all {
            self.http_notes.dur_req_send_all = dur;
        }
        if let Some(dur) = adaptation_state.dur_ups_recv_header {
            self.http_notes.dur_rsp_recv_hdr = dur;
        }
        self.http_notes.clt_req_body_size = adaptation_state.clt_req_body_size;
        self.http_notes.ups_req_body_size = adaptation_state.ups_req_body_size;
        match end_state {
            Ok(ReqmodAdaptationEndState::OriginalTransferred(ups_rsp))
            | Ok(ReqmodAdaptationEndState::AdaptedTransferred(_, ups_rsp)) => {
                self.send_response(
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

    pub(super) async fn send_adaptation_error_response(
        &mut self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        rsp: HttpAdapterErrorResponse,
        rsp_recv_body: Option<ReqmodRecvHttpResponseBody>,
    ) -> Result<(), H2StreamTransferError> {
        let mut parts = Response::new(()).into_parts().0;
        parts.version = Version::HTTP_2;
        parts.status = rsp.status;
        parts.headers = rsp.to_h2_headers();
        let response = Response::from_parts(parts, ());
        self.send_error_response = false;
        let rsp_status = response.status().as_u16();
        if let Some(mut recv_body) = rsp_recv_body {
            let mut clt_send_stream = clt_send_rsp
                .send_response(response, false)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            self.http_notes.rsp_status = rsp_status;
            let mut body_transfer = recv_body.body_transfer(&mut clt_send_stream);
            (&mut body_transfer).await.map_err(|e| {
                H2StreamTransferError::InternalAdapterError(anyhow::anyhow!(
                    "adapter error body: {e:?}"
                ))
            })?;
            self.http_notes.clt_rsp_body_size = Some(body_transfer.copied_size());
            recv_body.save_connection().await;
        } else {
            self.http_notes.clt_rsp_body_size = Some(0);
            clt_send_rsp
                .send_response(response, true)
                .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            self.http_notes.rsp_status = rsp_status;
        }
        Ok(())
    }

    async fn forward_without_adaptation(
        &mut self,
        origin: OriginH2Sender,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        let end_stream = clt_body.is_end_stream();
        let request_host = self.req.host();
        let opened = match self
            .ctx
            .open_h2_stream(
                &mut self.task_notes,
                &self.upstream,
                &request_host,
                origin,
                &self.req,
                end_stream,
            )
            .await
        {
            Ok(opened) => opened,
            Err(e) => {
                H2TaskContext::reset_unopened_stream(clt_send_rsp, &e);
                return Err(e);
            }
        };
        self.set_h2_origin_connected(opened.reused, opened.egress_notes);
        self.ups_stream_id = Some(opened.rsp_fut.stream_id());
        self.http_notes.mark_req_send_hdr();

        if end_stream {
            self.forward_without_body(opened.rsp_fut, clt_send_rsp)
                .await
        } else {
            self.forward_with_body(opened.rsp_fut, opened.send_stream, clt_body, clt_send_rsp)
                .await
        }
    }

    async fn forward_without_body(
        &mut self,
        ups_rsp_fut: ResponseFuture,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        self.http_notes.mark_req_no_body();

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(ups_rsp_fut);
        let ups_rsp = tokio::time::timeout(
            self.ctx.rsp_hdr_timeout(),
            self.recv_final_response(&mut ups_recv_rsp, clt_send_rsp),
        )
        .await
        .map_err(|_| H2StreamTransferError::ResponseHeadRecvTimeout)??;

        self.send_response(ups_rsp, clt_send_rsp, None).await
    }

    async fn forward_with_body(
        &mut self,
        ups_rsp_fut: ResponseFuture,
        ups_send_stream: SendStream<Bytes>,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        let mut req_body_transfer = H2BodyTransfer::new(
            clt_body,
            ups_send_stream,
            self.ctx.server_config.tcp_copy.yield_size(),
        );

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut idle_count = 0;

        let mut ups_rsp: Option<Response<RecvStream>> = None;
        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(ups_rsp_fut);

        macro_rules! record_progress {
            () => {
                self.http_notes.record_h2_req_body_progress(
                    req_body_transfer.received_size(),
                    req_body_transfer.copied_size(),
                )
            };
        }

        loop {
            tokio::select! {
                biased;

                r = &mut req_body_transfer => {
                    match r {
                        Ok(_) => {
                            self.http_notes.mark_req_send_all();
                            let n = req_body_transfer.copied_size();
                            self.http_notes.clt_req_body_size = Some(n);
                            self.http_notes.ups_req_body_size = Some(n);
                            break;
                        }
                        Err(e) => {
                            record_progress!();
                            return Err(H2StreamTransferError::RequestBodyTransferFailed(e));
                        }
                    }
                }
                r = ups_recv_rsp.recv_header() => {
                    match r {
                        Ok(rsp) => {
                            if let Some(final_rsp) = self.check_out_final_response(rsp, clt_send_rsp, &mut ups_recv_rsp)? {
                                record_progress!();
                                ups_rsp = Some(final_rsp);
                                break;
                            }
                        }
                        Err(e) => {
                            record_progress!();
                            return Err(H2StreamTransferError::ResponseHeadRecvFailed(e));
                        }
                    }
                }
                n = idle_interval.tick() => {
                    if req_body_transfer.is_idle() {
                        idle_count += n;
                        if idle_count > self.task_notes.task_max_idle_count(self.ctx.server_config.task_idle_max_count) {
                            record_progress!();
                            return Err(H2StreamTransferError::Idle(idle_interval.period(), idle_count));
                        }
                    } else {
                        idle_count = 0;
                        req_body_transfer.reset_active();
                    }
                    if self.ctx.server_quit_policy.force_quit() {
                        record_progress!();
                        return Err(H2StreamTransferError::CanceledAsServerQuit);
                    }
                }
            }
        }

        if let Some(ups_rsp) = ups_rsp {
            self.send_response(ups_rsp, clt_send_rsp, None).await
        } else {
            let ups_rsp = tokio::time::timeout(
                self.ctx.rsp_hdr_timeout(),
                self.recv_final_response(&mut ups_recv_rsp, clt_send_rsp),
            )
            .await
            .map_err(|_| H2StreamTransferError::ResponseHeadRecvTimeout)??;

            self.send_response(ups_rsp, clt_send_rsp, None).await
        }
    }

    async fn recv_final_response(
        &mut self,
        ups_recv_rsp: &mut H2ResponseHeaderReceiver,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<Response<RecvStream>, H2StreamTransferError> {
        loop {
            let rsp = ups_recv_rsp
                .recv_header()
                .await
                .map_err(H2StreamTransferError::ResponseHeadRecvFailed)?;
            if let Some(final_rsp) =
                self.check_out_final_response(rsp, clt_send_rsp, ups_recv_rsp)?
            {
                return Ok(final_rsp);
            }
        }
    }

    fn check_out_final_response(
        &mut self,
        rsp: Response<()>,
        clt_send_rsp: &mut SendResponse<Bytes>,
        ups_recv_rsp: &mut H2ResponseHeaderReceiver,
    ) -> Result<Option<Response<RecvStream>>, H2StreamTransferError> {
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
            StatusCode::PROCESSING | StatusCode::EARLY_HINTS => {
                clt_send_rsp
                    .send_informational(rsp)
                    .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            }
            status if status.is_informational() => {
                return Err(H2StreamTransferError::UnsupportedInformationalResponse(
                    status,
                ));
            }
            status => {
                self.http_notes.mark_rsp_recv_hdr();
                return if let Some(body) = ups_recv_rsp.take_body() {
                    let (headers, _) = rsp.into_parts();
                    Ok(Some(Response::from_parts(headers, body)))
                } else {
                    Err(H2StreamTransferError::UnsupportedInformationalResponse(
                        status,
                    ))
                };
            }
        }
        Ok(None)
    }

    async fn send_response(
        &mut self,
        ups_rsp: Response<RecvStream>,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adaptation_respond_shared_headers: Option<HeaderMap>,
    ) -> Result<(), H2StreamTransferError> {
        let (parts, ups_body) = ups_rsp.into_parts();
        let clt_rsp = Response::from_parts(parts, ());
        self.http_notes.origin_status = clt_rsp.status().as_u16();

        if self.audit_task
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
                        self.task_notes.task_created_instant(),
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
                    let r = adapter
                        .xfer(
                            &mut adaptation_state,
                            &self.req,
                            clt_rsp,
                            ups_body,
                            clt_send_rsp,
                        )
                        .await;
                    if let Some(dur) = adaptation_state.dur_ups_recv_all {
                        self.http_notes.dur_rsp_recv_all = dur;
                    }
                    self.http_notes.ups_rsp_body_size = adaptation_state.ups_rsp_body_size;
                    self.http_notes.clt_rsp_body_size = adaptation_state.clt_rsp_body_size;
                    match r {
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

        macro_rules! record_progress {
            () => {
                self.http_notes.record_h2_rsp_body_progress(
                    rsp_body_transfer.received_size(),
                    rsp_body_transfer.copied_size(),
                )
            };
        }

        loop {
            tokio::select! {
                biased;
                r = &mut rsp_body_transfer => {
                    match r {
                        Ok(_) => {
                            self.http_notes.mark_rsp_recv_all();
                            let n = rsp_body_transfer.copied_size();
                            self.http_notes.ups_rsp_body_size = Some(n);
                            self.http_notes.clt_rsp_body_size = Some(n);
                            return Ok(());
                        }
                        Err(e) => {
                            record_progress!();
                            return Err(H2StreamTransferError::ResponseBodyTransferFailed(e));
                        }
                    }
                }
                n = idle_interval.tick() => {
                    if rsp_body_transfer.is_idle() {
                        idle_count += n;
                        if idle_count > self.task_notes.task_max_idle_count(self.ctx.server_config.task_idle_max_count) {
                            record_progress!();
                            return Err(H2StreamTransferError::Idle(idle_interval.period(), idle_count));
                        }
                    } else {
                        idle_count = 0;
                        rsp_body_transfer.reset_active();
                    }
                    if self.ctx.server_quit_policy.force_quit() {
                        record_progress!();
                        return Err(H2StreamTransferError::CanceledAsServerQuit);
                    }
                }
            }
        }
    }
}
