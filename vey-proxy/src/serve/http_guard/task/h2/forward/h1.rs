/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use bytes::Bytes;
use futures_util::FutureExt;
use h2::server::SendResponse;
use h2::{RecvStream, SendStream};
use http::{HeaderMap, Response};
use tokio::io::AsyncWriteExt;

use vey_h2::{H2BodyEncodeTransfer, H2StreamFromChunkedTransfer, H2StreamToChunkedTransfer};
use vey_http::client::HttpForwardRemoteResponse;
use vey_http::server::HttpConvertedRequest;
use vey_http::{HttpBodyDecodeReader, HttpBodyType};
use vey_icap_client::reqmod::h1::HttpRequestUpstreamWriter;
use vey_icap_client::reqmod::h2_to_h1::{
    H2ToH1RequestAdapter, ReqmodAdaptationEndState, ReqmodAdaptationRunState,
};
use vey_icap_client::respmod::h1_to_h2::{
    H1ToH2ResponseAdapter, RespmodAdaptationEndState, RespmodAdaptationRunState,
};
use vey_io_ext::LimitedBufReadExt;

use super::OriginH1Sender;
use super::task::H2ForwardTask;
use crate::module::http_forward::{
    BoxHttpForwardConnection, BoxHttpForwardReader, HttpForwardWriterForAdaptation,
};
use crate::serve::{ServerIdleChecker, ServerTaskStage};
use crate::serve::{ServerTaskError, ServerTaskResult};

impl H2ForwardTask {
    pub(super) async fn forward_h1_origin(
        &mut self,
        mut origin: OriginH1Sender,
        mut clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> ServerTaskResult<()> {
        let converted = HttpConvertedRequest::from_request(&self.req, !clt_body.is_end_stream())
            .map_err(|e| {
                ServerTaskError::InternalAdapterError(anyhow!("invalid converted request: {e}"))
            })?;
        let no_body = clt_body.is_end_stream();

        self.prepare_h1_origin(&mut origin);
        let keep_alive = match self
            .run_with_h1_connection(
                &mut origin,
                &converted,
                &mut clt_body,
                clt_send_rsp,
                no_body,
            )
            .await
        {
            Ok(keep_alive) => keep_alive,
            Err(e) => {
                origin = self.reconnect_after_stale_h1(e).await?;
                self.prepare_h1_origin(&mut origin);
                self.run_with_h1_connection(
                    &mut origin,
                    &converted,
                    &mut clt_body,
                    clt_send_rsp,
                    no_body,
                )
                .await?
            }
        };
        if keep_alive {
            self.save_h1_origin(origin);
        }
        Ok(())
    }

    async fn run_with_h1_connection(
        &mut self,
        origin: &mut OriginH1Sender,
        converted: &HttpConvertedRequest,
        clt_body: &mut RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
        no_body: bool,
    ) -> ServerTaskResult<bool> {
        if let Some(e) = self.poll_idle_h1_origin(origin) {
            return Err(e);
        }

        self.mark_relaying();

        if self.audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(reqmod) = audit_handle.icap_reqmod_client()
        {
            match reqmod
                .h2_to_h1_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.server_config.h2.max_header_list_size as usize,
                    true,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    adapter.set_client_addr(self.task_notes.client_addr());
                    if let Some(username) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(username.clone());
                    }
                    if let Some(username) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(username.clone());
                    }
                    self.forward_h1_with_adaptation(
                        origin,
                        converted,
                        clt_body,
                        clt_send_rsp,
                        adapter,
                        no_body,
                    )
                    .await
                }
                Err(e) => {
                    if !reqmod.bypass() {
                        return Err(ServerTaskError::InternalAdapterError(e));
                    }
                    self.forward_h1_without_adaptation(origin, converted, clt_body, clt_send_rsp)
                        .await
                }
            }
        } else {
            self.forward_h1_without_adaptation(origin, converted, clt_body, clt_send_rsp)
                .await
        }
    }

    fn prepare_h1_origin(&mut self, origin: &mut OriginH1Sender) {
        self.http_notes.reused_connection = origin.reused;
        self.http_notes.retry_new_connection = false;
        self.egress_notes = origin.egress_notes.clone();
        self.task_notes.stage = ServerTaskStage::Connected;
        origin
            .connection
            .0
            .prepare_new(&self.task_notes, &self.upstream);
    }

    fn poll_idle_h1_origin(&mut self, origin: &mut OriginH1Sender) -> Option<ServerTaskError> {
        if !origin.reused {
            return None;
        }
        let r = origin.connection.1.fill_wait_data().now_or_never()?;
        self.http_notes.retry_new_connection = true;
        Some(match r {
            Ok(true) => ServerTaskError::InternalServerError(
                "unexpected data found when polling idle origin connection",
            ),
            Ok(false) => ServerTaskError::ClosedByUpstream,
            Err(e) => ServerTaskError::UpstreamReadFailed(e),
        })
    }

    async fn reconnect_after_stale_h1(
        &mut self,
        e: ServerTaskError,
    ) -> ServerTaskResult<OriginH1Sender> {
        if !(self.http_notes.reused_connection && self.http_notes.retry_new_connection) {
            return Err(e);
        }
        if let Some(log) = self.log_ctx() {
            log.log(&e);
        }
        self.http_notes.retry_new_connection = false;
        self.task_notes.stage = ServerTaskStage::Connecting;
        self.ctx
            .connect_origin_h1(&self.task_notes, &self.upstream, &self.req_host)
            .await
    }

    async fn forward_h1_with_adaptation(
        &mut self,
        origin: &mut OriginH1Sender,
        converted: &HttpConvertedRequest,
        clt_body: &mut RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adapter: H2ToH1RequestAdapter<ServerIdleChecker>,
        no_body: bool,
    ) -> ServerTaskResult<bool> {
        self.http_notes.retry_new_connection = no_body;
        let mut adaptation_state =
            ReqmodAdaptationRunState::new(self.task_notes.task_created_instant());
        let mut rsp_header = None;
        let icap_error = {
            let ups_w = &mut origin.connection.0;
            let ups_r = &mut origin.connection.1;
            let mut ups_w_adaptation =
                HttpForwardWriterForAdaptation::new(ups_w, origin.egress_notes.expire_at);
            let adaptation_fut = adapter.xfer(
                &mut adaptation_state,
                converted,
                clt_body,
                &mut ups_w_adaptation,
            );
            tokio::pin!(adaptation_fut);

            let mut icap_error = None;
            loop {
                tokio::select! {
                    biased;
                    r = ups_r.fill_wait_data() => {
                        match r {
                            Ok(true) => {
                                let hdr = self.recv_h1_response_header_in_time(ups_r).await?;
                                if let Some(final_hdr) =
                                    self.check_out_h1_informational(hdr, clt_send_rsp)?
                                {
                                    rsp_header = Some(final_hdr);
                                    break;
                                }
                            }
                            Ok(false) => {
                                if no_body {
                                    self.http_notes.retry_new_connection = true;
                                }
                                return Err(ServerTaskError::ClosedByUpstream);
                            }
                            Err(e) => {
                                if no_body {
                                    self.http_notes.retry_new_connection = true;
                                }
                                return Err(ServerTaskError::UpstreamReadFailed(e));
                            }
                        }
                    }
                    r = &mut adaptation_fut => {
                        match r {
                            Ok(ReqmodAdaptationEndState::OriginalTransferred)
                            | Ok(ReqmodAdaptationEndState::AdaptedTransferred(_)) => break,
                            Ok(ReqmodAdaptationEndState::HttpErrResponse(err_rsp, recv_body)) => {
                                self.http_notes.retry_new_connection = false;
                                icap_error = Some((err_rsp, recv_body));
                                break;
                            }
                            Err(e) => {
                                let err = ServerTaskError::from(e);
                                if no_body {
                                    self.http_notes.retry_new_connection = matches!(
                                        err,
                                        ServerTaskError::UpstreamWriteFailed(_)
                                            | ServerTaskError::ClosedByUpstream
                                            | ServerTaskError::UpstreamReadFailed(_)
                                    );
                                }
                                return Err(err);
                            }
                        }
                    }
                }
            }
            icap_error
        };
        if let Some((err_rsp, recv_body)) = icap_error {
            self.send_adaptation_error_response(clt_send_rsp, err_rsp, recv_body)
                .await?;
            return Ok(false);
        }

        self.http_notes.clt_req_body_size = adaptation_state.clt_req_body_size;
        self.http_notes.ups_req_body_size = adaptation_state.ups_req_body_size;
        if let Some(d) = adaptation_state.dur_ups_send_header {
            self.http_notes.dur_req_send_hdr = d;
        }
        if let Some(d) = adaptation_state.dur_ups_send_all {
            self.http_notes.dur_req_send_all = d;
        }

        let mut rsp_header = match self
            .recv_final_h1_response(&mut origin.connection, clt_send_rsp, rsp_header)
            .await
        {
            Ok(header) => {
                self.http_notes.retry_new_connection = false;
                header
            }
            Err(e) => {
                self.retain_retry_on_recv_error(&e);
                return Err(e);
            }
        };
        let keep_alive = rsp_header.keep_alive()
            && adaptation_state.ups_write_finished
            && !rsp_header.www_negotiate_auth();
        if keep_alive {
            origin
                .reuse_notes
                .overlay_keep_alive(rsp_header.keep_alive_header());
        }

        let ups_read_finished = self
            .send_h1_origin_response(
                &mut rsp_header,
                &mut origin.connection.1,
                clt_send_rsp,
                adaptation_state.take_respond_shared_headers(),
            )
            .await?;
        Ok(keep_alive && ups_read_finished)
    }

    async fn forward_h1_without_adaptation(
        &mut self,
        origin: &mut OriginH1Sender,
        converted: &HttpConvertedRequest,
        clt_body: &mut RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> ServerTaskResult<bool> {
        let mut rsp_header = None;
        self.http_notes.retry_new_connection = true;
        let (clt_read_finished, ups_write_finished) = {
            let ups_w = &mut origin.connection.0;
            let ups_r = &mut origin.connection.1;
            let mut ups_w_adaptation =
                HttpForwardWriterForAdaptation::new(ups_w, origin.egress_notes.expire_at);
            ups_w_adaptation
                .send_request_header(converted)
                .await
                .map_err(ServerTaskError::UpstreamWriteFailed)?;
            self.http_notes.mark_req_send_hdr();

            if clt_body.is_end_stream() {
                ups_w_adaptation
                    .flush()
                    .await
                    .map_err(ServerTaskError::UpstreamWriteFailed)?;
                self.http_notes.mark_req_no_body();
                (true, true)
            } else {
                self.http_notes.retry_new_connection = false;
                let mut body_transfer = H2StreamToChunkedTransfer::new(
                    clt_body,
                    ups_w,
                    self.ctx.server_config.tcp_copy.yield_size(),
                );
                let mut idle_interval = self.ctx.idle_wheel.register();
                let mut idle_count = 0;
                macro_rules! record_req_body {
                    () => {
                        self.http_notes.clt_req_body_size = Some(body_transfer.received_size());
                        self.http_notes.ups_req_body_size = Some(body_transfer.copied_size());
                    };
                }
                loop {
                    tokio::select! {
                        biased;
                        r = ups_r.fill_wait_data() => {
                            match r {
                                Ok(true) => {
                                    let hdr = self.recv_h1_response_header_in_time(ups_r).await?;
                                    if let Some(final_hdr) =
                                        self.check_out_h1_informational(hdr, clt_send_rsp)?
                                    {
                                        record_req_body!();
                                        rsp_header = Some(final_hdr);
                                        break (body_transfer.recv_finished(), body_transfer.finished());
                                    }
                                }
                                Ok(false) => {
                                    if body_transfer.received_size() == 0 {
                                        self.http_notes.retry_new_connection = true;
                                    }
                                    record_req_body!();
                                    return Err(ServerTaskError::ClosedByUpstream);
                                }
                                Err(e) => {
                                    if body_transfer.received_size() == 0 {
                                        self.http_notes.retry_new_connection = true;
                                    }
                                    record_req_body!();
                                    return Err(ServerTaskError::UpstreamReadFailed(e));
                                }
                            }
                        }
                        r = &mut body_transfer => {
                            match r {
                                Ok(n) => {
                                    self.http_notes.mark_req_send_all();
                                    self.http_notes.clt_req_body_size = Some(n);
                                    self.http_notes.ups_req_body_size = Some(n);
                                    break (true, true);
                                }
                                Err(e) => {
                                    record_req_body!();
                                    return Err(ServerTaskError::request_h2_to_chunked_error(e));
                                }
                            }
                        }
                        n = idle_interval.tick() => {
                            if body_transfer.is_idle() {
                                idle_count += n;
                                if idle_count
                                    > self.task_notes.task_max_idle_count(
                                        self.ctx.server_config.task_idle_max_count,
                                    )
                                {
                                    record_req_body!();
                                    return Err(ServerTaskError::Idle(
                                        idle_interval.period(),
                                        idle_count,
                                    ));
                                }
                            } else {
                                idle_count = 0;
                                body_transfer.reset_active();
                            }
                            if self.ctx.server_quit_policy.force_quit() {
                                record_req_body!();
                                return Err(ServerTaskError::CanceledAsServerQuit);
                            }
                        }
                    }
                }
            }
        };

        let mut rsp_header = match self
            .recv_final_h1_response(&mut origin.connection, clt_send_rsp, rsp_header)
            .await
        {
            Ok(header) => {
                self.http_notes.retry_new_connection = false;
                header
            }
            Err(e) => {
                self.retain_retry_on_recv_error(&e);
                return Err(e);
            }
        };
        let keep_alive = rsp_header.keep_alive()
            && clt_read_finished
            && ups_write_finished
            && !rsp_header.www_negotiate_auth();
        if keep_alive {
            origin
                .reuse_notes
                .overlay_keep_alive(rsp_header.keep_alive_header());
        }

        let ups_read_finished = self
            .send_h1_origin_response(
                &mut rsp_header,
                &mut origin.connection.1,
                clt_send_rsp,
                None,
            )
            .await?;
        Ok(keep_alive && ups_read_finished)
    }

    fn retain_retry_on_recv_error(&mut self, e: &ServerTaskError) {
        if !self.http_notes.retry_new_connection {
            return;
        }
        self.http_notes.retry_new_connection = matches!(
            e,
            ServerTaskError::ClosedByUpstream | ServerTaskError::UpstreamReadFailed(_)
        );
    }

    async fn recv_final_h1_response(
        &mut self,
        ups_c: &mut BoxHttpForwardConnection,
        clt_send_rsp: &mut SendResponse<Bytes>,
        rsp_header: Option<HttpForwardRemoteResponse>,
    ) -> ServerTaskResult<HttpForwardRemoteResponse> {
        let rsp = match rsp_header {
            Some(header) => header,
            None => tokio::time::timeout(self.ctx.rsp_hdr_timeout(), async {
                loop {
                    let hdr = self.recv_h1_response_header(&mut ups_c.1).await?;
                    if let Some(final_hdr) = self.check_out_h1_informational(hdr, clt_send_rsp)? {
                        return Ok::<_, ServerTaskError>(final_hdr);
                    }
                }
            })
            .await
            .map_err(|_| ServerTaskError::UpstreamAppTimeout("timeout to recv response head"))??,
        };
        self.http_notes.mark_rsp_recv_hdr();
        self.http_notes.origin_status = rsp.code;
        Ok(rsp)
    }

    /// For headers that start arriving while the request body is still being
    /// sent: the select loop polls nothing else until the header completes.
    async fn recv_h1_response_header_in_time(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
    ) -> ServerTaskResult<HttpForwardRemoteResponse> {
        tokio::time::timeout(
            self.ctx.rsp_hdr_timeout(),
            self.recv_h1_response_header(ups_r),
        )
        .await
        .map_err(|_| ServerTaskError::UpstreamAppTimeout("timeout to recv response head"))?
    }

    async fn recv_h1_response_header(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
    ) -> ServerTaskResult<HttpForwardRemoteResponse> {
        let method = self.http_notes.method.clone();
        Ok(ups_r
            .recv_response_header(
                &method,
                true,
                self.ctx.server_config.rsp_hdr_max_size,
                &mut self.http_notes,
            )
            .await?)
    }

    fn check_out_h1_informational(
        &mut self,
        hdr: HttpForwardRemoteResponse,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> ServerTaskResult<Option<HttpForwardRemoteResponse>> {
        match hdr.code {
            100 => {
                if self.allow_continue {
                    clt_send_rsp
                        .send_informational(hdr.to_h2_response())
                        .map_err(|e| {
                            ServerTaskError::ClientAppError(anyhow!(
                                "send h2 informational response to client failed: {e}"
                            ))
                        })?;
                    self.allow_continue = false;
                } else {
                    return Err(ServerTaskError::invalid_upstream_100_continue_response());
                }
            }
            102 | 103 => {
                clt_send_rsp
                    .send_informational(hdr.to_h2_response())
                    .map_err(|e| {
                        ServerTaskError::ClientAppError(anyhow!(
                            "send h2 informational response to client failed: {e}"
                        ))
                    })?;
            }
            // 101 is not allowed in HTTP/2, and other 1xx are unknown here.
            101 | 104..200 => {
                return Err(ServerTaskError::UpstreamAppError(anyhow!(
                    "unsupported h2 informational response {}",
                    hdr.to_h2_response().status()
                )));
            }
            _ => return Ok(Some(hdr)),
        }
        Ok(None)
    }

    async fn send_h1_origin_response(
        &mut self,
        rsp_header: &mut HttpForwardRemoteResponse,
        ups_r: &mut BoxHttpForwardReader,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adaptation_respond_shared_headers: Option<HeaderMap>,
    ) -> ServerTaskResult<bool> {
        let body_type = rsp_header.body_type(&self.http_notes.method);
        let clt_rsp = rsp_header.to_h2_response();

        if self.audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(respmod) = audit_handle.icap_respmod_client()
        {
            match respmod
                .h1_to_h2_adapter(
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
                    return self
                        .send_response_with_adaptation(
                            clt_rsp,
                            body_type,
                            ups_r,
                            clt_send_rsp,
                            adapter,
                            &mut adaptation_state,
                        )
                        .await;
                }
                Err(e) => {
                    if !respmod.bypass() {
                        return Err(ServerTaskError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.send_response_without_adaptation(clt_rsp, body_type, ups_r, clt_send_rsp)
            .await
    }

    async fn send_response_with_adaptation(
        &mut self,
        clt_rsp: Response<()>,
        body_type: Option<HttpBodyType>,
        ups_r: &mut BoxHttpForwardReader,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adapter: H1ToH2ResponseAdapter<ServerIdleChecker>,
        adaptation_state: &mut RespmodAdaptationRunState,
    ) -> ServerTaskResult<bool> {
        let r = adapter
            .xfer(
                adaptation_state,
                &self.req,
                clt_rsp,
                body_type,
                ups_r,
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
                self.send_error_response = false;
                Ok(adaptation_state.ups_read_finished)
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn send_response_without_adaptation(
        &mut self,
        clt_rsp: Response<()>,
        body_type: Option<HttpBodyType>,
        ups_r: &mut BoxHttpForwardReader,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> ServerTaskResult<bool> {
        self.send_error_response = false;
        match body_type {
            None => {
                self.http_notes.mark_rsp_no_body();
                clt_send_rsp.send_response(clt_rsp, true).map_err(|e| {
                    ServerTaskError::ClientAppError(anyhow!(
                        "send h2 response to client failed: {e}"
                    ))
                })?;
                self.http_notes.rsp_status = self.http_notes.origin_status;
                Ok(true)
            }
            Some(body_type) => {
                let mut clt_send_stream =
                    clt_send_rsp.send_response(clt_rsp, false).map_err(|e| {
                        ServerTaskError::ClientAppError(anyhow!(
                            "send h2 response to client failed: {e}"
                        ))
                    })?;
                self.http_notes.rsp_status = self.http_notes.origin_status;
                self.send_response_body(ups_r, &mut clt_send_stream, body_type)
                    .await?;
                Ok(true)
            }
        }
    }

    async fn send_response_body(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
        clt_send_stream: &mut SendStream<Bytes>,
        body_type: HttpBodyType,
    ) -> ServerTaskResult<()> {
        match body_type {
            HttpBodyType::Chunked => {
                let mut body_transfer = H2StreamFromChunkedTransfer::new(
                    ups_r,
                    clt_send_stream,
                    &self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.server_config.h2.max_header_list_size as usize,
                );
                let mut idle_interval = self.ctx.idle_wheel.register();
                let mut idle_count = 0;
                loop {
                    tokio::select! {
                        biased;
                        r = &mut body_transfer => {
                            match r {
                                Ok(_) => break,
                                Err(e) => {
                                    self.http_notes.ups_rsp_body_size =
                                        Some(body_transfer.received_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(ServerTaskError::response_chunked_to_h2_error(e));
                                }
                            }
                        }
                        n = idle_interval.tick() => {
                            if body_transfer.is_idle() {
                                idle_count += n;
                                if idle_count
                                    > self.task_notes.task_max_idle_count(
                                        self.ctx.server_config.task_idle_max_count,
                                    )
                                {
                                    self.http_notes.ups_rsp_body_size =
                                        Some(body_transfer.received_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(ServerTaskError::Idle(
                                        idle_interval.period(),
                                        idle_count,
                                    ));
                                }
                            } else {
                                idle_count = 0;
                                body_transfer.reset_active();
                            }
                            if self.ctx.server_quit_policy.force_quit() {
                                self.http_notes.ups_rsp_body_size =
                                    Some(body_transfer.received_size());
                                self.http_notes.clt_rsp_body_size =
                                    Some(body_transfer.copied_size());
                                return Err(ServerTaskError::CanceledAsServerQuit);
                            }
                        }
                    }
                }
                let n = body_transfer.copied_size();
                self.http_notes.ups_rsp_body_size = Some(n);
                self.http_notes.clt_rsp_body_size = Some(n);
                self.http_notes.mark_rsp_recv_all();
                Ok(())
            }
            HttpBodyType::ContentLength(_) | HttpBodyType::ReadUntilEnd => {
                let mut body_reader = HttpBodyDecodeReader::new(
                    ups_r,
                    body_type,
                    self.ctx.server_config.h1.body_line_max_len,
                );
                let mut body_transfer = H2BodyEncodeTransfer::new(
                    &mut body_reader,
                    clt_send_stream,
                    &self.ctx.server_config.tcp_copy,
                );
                let mut idle_interval = self.ctx.idle_wheel.register();
                let mut idle_count = 0;
                loop {
                    tokio::select! {
                        biased;
                        r = &mut body_transfer => {
                            match r {
                                Ok(_) => break,
                                Err(e) => {
                                    self.http_notes.ups_rsp_body_size =
                                        Some(body_transfer.received_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(ServerTaskError::response_h2_encode_error(e));
                                }
                            }
                        }
                        n = idle_interval.tick() => {
                            if body_transfer.is_idle() {
                                idle_count += n;
                                if idle_count
                                    > self.task_notes.task_max_idle_count(
                                        self.ctx.server_config.task_idle_max_count,
                                    )
                                {
                                    self.http_notes.ups_rsp_body_size =
                                        Some(body_transfer.received_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(ServerTaskError::Idle(
                                        idle_interval.period(),
                                        idle_count,
                                    ));
                                }
                            } else {
                                idle_count = 0;
                                body_transfer.reset_active();
                            }
                            if self.ctx.server_quit_policy.force_quit() {
                                self.http_notes.ups_rsp_body_size =
                                    Some(body_transfer.received_size());
                                self.http_notes.clt_rsp_body_size =
                                    Some(body_transfer.copied_size());
                                return Err(ServerTaskError::CanceledAsServerQuit);
                            }
                        }
                    }
                }
                let n = body_transfer.copied_size();
                drop(body_transfer);
                self.http_notes.mark_rsp_recv_all();
                clt_send_stream.send_data(Bytes::new(), true).map_err(|e| {
                    ServerTaskError::ClientAppError(anyhow!("send h2 data to client failed: {e}"))
                })?;
                self.http_notes.ups_rsp_body_size = Some(n);
                self.http_notes.clt_rsp_body_size = Some(n);
                Ok(())
            }
        }
    }

    fn save_h1_origin(&self, origin: OriginH1Sender) {
        let site = self.ctx.site_ctx.site();
        if !site.h1_keepalive_config().is_enabled() {
            return;
        }
        let Some(pool) = site.http1_pool() else {
            return;
        };
        pool.save(
            self.task_notes.worker_id(),
            self.ctx.escaper.name().clone(),
            self.upstream.socket_addr(),
            origin.connection,
            origin.reuse_notes,
            origin.egress_notes,
        );
    }
}
