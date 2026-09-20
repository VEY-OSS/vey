/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use bytes::Bytes;
use h2::server::SendResponse;
use h2::{RecvStream, SendStream};
use http::Request;
use tokio::time::Instant;

use vey_h2::{
    H2BodyEncodeTransfer, H2StreamBodyTransferError, H2StreamFromChunkedTransfer,
    H2StreamToChunkedTransfer,
};
use vey_http::client::HttpForwardRemoteResponse;
use vey_http::server::HttpConvertedRequest;
use vey_http::{HttpBodyDecodeReader, HttpBodyType};
use vey_icap_client::reqmod::h1::HttpRequestUpstreamWriter;
use vey_icap_client::reqmod::h2_to_h1::{
    H2ToH1RequestAdapter, ReqmodAdaptationEndState, ReqmodAdaptationRunState,
};
use vey_icap_client::respmod::h1_to_h2::{RespmodAdaptationEndState, RespmodAdaptationRunState};
use vey_io_ext::LimitedBufReadExt;

use super::task::H2ForwardTask;
use super::{H2StreamTransferError, OriginH1Sender};
use crate::module::http_forward::{
    BoxHttpForwardConnection, BoxHttpForwardReader, HttpForwardWriterForAdaptation,
};
use crate::serve::ServerTaskStage;

impl H2ForwardTask {
    pub(super) async fn forward_h1_origin(
        &mut self,
        mut origin: OriginH1Sender,
        clt_req: Request<RecvStream>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<(), H2StreamTransferError> {
        self.http_notes.reused_connection = origin.reused;
        self.egress_notes = origin.egress_notes.clone();
        self.task_notes.stage = ServerTaskStage::Connected;
        origin
            .connection
            .0
            .prepare_new(&self.task_notes, self.ctx.site_ctx.site().upstream());

        let (parts, clt_body) = clt_req.into_parts();
        let orig_req = Request::from_parts(parts, ());
        let converted = HttpConvertedRequest::from_request(&orig_req, !clt_body.is_end_stream())?;

        let keep_alive = if self.audit_task
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
                        &mut origin,
                        orig_req,
                        converted,
                        clt_body,
                        clt_send_rsp,
                        adapter,
                    )
                    .await?
                }
                Err(e) => {
                    if !reqmod.bypass() {
                        return Err(H2StreamTransferError::InternalAdapterError(e));
                    }
                    self.forward_h1_without_adaptation(
                        &mut origin,
                        orig_req,
                        converted,
                        clt_body,
                        clt_send_rsp,
                    )
                    .await?
                }
            }
        } else {
            self.forward_h1_without_adaptation(
                &mut origin,
                orig_req,
                converted,
                clt_body,
                clt_send_rsp,
            )
            .await?
        };

        if keep_alive {
            self.save_h1_origin(origin);
        }
        Ok(())
    }

    async fn forward_h1_with_adaptation(
        &mut self,
        origin: &mut OriginH1Sender,
        orig_req: Request<()>,
        converted: HttpConvertedRequest,
        clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adapter: H2ToH1RequestAdapter<crate::serve::ServerIdleChecker>,
    ) -> Result<bool, H2StreamTransferError> {
        let mut adaptation_state = ReqmodAdaptationRunState::new(Instant::now());
        let mut rsp_header = None;
        let icap_error = {
            let ups_w = &mut origin.connection.0;
            let ups_r = &mut origin.connection.1;
            let mut ups_w_adaptation = HttpForwardWriterForAdaptation { inner: ups_w };
            let adaptation_fut = adapter.xfer(
                &mut adaptation_state,
                &converted,
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
                                let hdr = self.recv_h1_response_header(ups_r).await?;
                                if let Some(final_hdr) =
                                    self.check_out_h1_informational(hdr, clt_send_rsp)?
                                {
                                    rsp_header = Some(final_hdr);
                                    break;
                                }
                            }
                            Ok(false) => return Err(H2StreamTransferError::OriginClosed),
                            Err(e) => return Err(H2StreamTransferError::OriginReadFailed(e)),
                        }
                    }
                    r = &mut adaptation_fut => {
                        match r {
                            Ok(ReqmodAdaptationEndState::OriginalTransferred)
                            | Ok(ReqmodAdaptationEndState::AdaptedTransferred(_)) => break,
                            Ok(ReqmodAdaptationEndState::HttpErrResponse(err_rsp, recv_body)) => {
                                icap_error = Some((err_rsp, recv_body));
                                break;
                            }
                            Err(e) => return Err(e.into()),
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

        let mut rsp_header = self
            .recv_final_h1_response(&mut origin.connection, clt_send_rsp, rsp_header)
            .await?;
        let keep_alive = rsp_header.keep_alive()
            && adaptation_state.clt_read_finished
            && adaptation_state.ups_write_finished
            && !rsp_header.www_negotiate_auth();
        if keep_alive {
            origin
                .reuse_notes
                .overlay_keep_alive(rsp_header.keep_alive_header());
        }

        let ups_read_finished = self
            .send_h1_origin_response(
                orig_req,
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
        orig_req: Request<()>,
        converted: HttpConvertedRequest,
        mut clt_body: RecvStream,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<bool, H2StreamTransferError> {
        let mut rsp_header = None;
        let (clt_read_finished, ups_write_finished) = {
            let ups_w = &mut origin.connection.0;
            let ups_r = &mut origin.connection.1;
            let mut ups_w_adaptation = HttpForwardWriterForAdaptation { inner: ups_w };
            ups_w_adaptation
                .send_request_header(&converted)
                .await
                .map_err(H2StreamTransferError::OriginWriteFailed)?;
            self.http_notes.mark_req_send_hdr();

            if clt_body.is_end_stream() {
                self.http_notes.mark_req_no_body();
                (true, true)
            } else {
                let mut body_transfer = H2StreamToChunkedTransfer::new(
                    &mut clt_body,
                    ups_w,
                    self.ctx.server_config.tcp_copy.yield_size(),
                );
                let mut idle_interval = self.ctx.idle_wheel.register();
                let mut idle_count = 0;
                loop {
                    tokio::select! {
                        biased;
                        r = ups_r.fill_wait_data() => {
                            match r {
                                Ok(true) => {
                                    let hdr = self.recv_h1_response_header(ups_r).await?;
                                    if let Some(final_hdr) =
                                        self.check_out_h1_informational(hdr, clt_send_rsp)?
                                    {
                                        rsp_header = Some(final_hdr);
                                        break (body_transfer.recv_finished(), body_transfer.finished());
                                    }
                                }
                                Ok(false) => {
                                    if body_transfer.copied_size() == 0 {
                                        self.http_notes.retry_new_connection = true;
                                    }
                                    return Err(H2StreamTransferError::OriginClosed);
                                }
                                Err(e) => {
                                    if body_transfer.copied_size() == 0 {
                                        self.http_notes.retry_new_connection = true;
                                    }
                                    return Err(H2StreamTransferError::OriginReadFailed(e));
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
                                Err(e) => return Err(e.into()),
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
                                    return Err(H2StreamTransferError::Idle(
                                        idle_interval.period(),
                                        idle_count,
                                    ));
                                }
                            } else {
                                idle_count = 0;
                                body_transfer.reset_active();
                            }
                            if self.ctx.server_quit_policy.force_quit() {
                                return Err(H2StreamTransferError::CanceledAsServerQuit);
                            }
                        }
                    }
                }
            }
        };

        let mut rsp_header = self
            .recv_final_h1_response(&mut origin.connection, clt_send_rsp, rsp_header)
            .await?;
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
                orig_req,
                &mut rsp_header,
                &mut origin.connection.1,
                clt_send_rsp,
                None,
            )
            .await?;
        Ok(keep_alive && ups_read_finished)
    }

    async fn recv_final_h1_response(
        &mut self,
        ups_c: &mut BoxHttpForwardConnection,
        clt_send_rsp: &mut SendResponse<Bytes>,
        rsp_header: Option<HttpForwardRemoteResponse>,
    ) -> Result<HttpForwardRemoteResponse, H2StreamTransferError> {
        let rsp = match rsp_header {
            Some(header) => header,
            None => tokio::time::timeout(self.rsp_hdr_timeout(), async {
                loop {
                    let hdr = self.recv_h1_response_header(&mut ups_c.1).await?;
                    if let Some(final_hdr) = self.check_out_h1_informational(hdr, clt_send_rsp)? {
                        return Ok::<_, H2StreamTransferError>(final_hdr);
                    }
                }
            })
            .await
            .map_err(|_| H2StreamTransferError::ResponseHeadRecvTimeout)??,
        };
        self.http_notes.mark_rsp_recv_hdr();
        self.http_notes.origin_status = rsp.code;
        Ok(rsp)
    }

    async fn recv_h1_response_header(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
    ) -> Result<HttpForwardRemoteResponse, H2StreamTransferError> {
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
    ) -> Result<Option<HttpForwardRemoteResponse>, H2StreamTransferError> {
        match hdr.code {
            100 => {
                if self.allow_continue {
                    clt_send_rsp
                        .send_informational(hdr.to_h2_response())
                        .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
                    self.allow_continue = false;
                } else {
                    return Err(H2StreamTransferError::InvalidContinueResponse);
                }
            }
            103 => {
                clt_send_rsp
                    .send_informational(hdr.to_h2_response())
                    .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
            }
            _ => return Ok(Some(hdr)),
        }
        Ok(None)
    }

    async fn send_h1_origin_response(
        &mut self,
        orig_req: Request<()>,
        rsp_header: &mut HttpForwardRemoteResponse,
        ups_r: &mut BoxHttpForwardReader,
        clt_send_rsp: &mut SendResponse<Bytes>,
        adaptation_respond_shared_headers: Option<http::HeaderMap>,
    ) -> Result<bool, H2StreamTransferError> {
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
                    self.rsp_hdr_timeout(),
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
                    let r = adapter
                        .xfer(
                            &mut adaptation_state,
                            &orig_req,
                            clt_rsp,
                            body_type,
                            ups_r,
                            clt_send_rsp,
                        )
                        .await;
                    self.http_notes.ups_rsp_body_size = adaptation_state.ups_rsp_body_size;
                    self.http_notes.clt_rsp_body_size = adaptation_state.clt_rsp_body_size;
                    match r {
                        Ok(RespmodAdaptationEndState::OriginalTransferred)
                        | Ok(RespmodAdaptationEndState::AdaptedTransferred(_)) => {
                            self.http_notes.rsp_status = self.http_notes.origin_status;
                            self.send_error_response = false;
                            return Ok(adaptation_state.ups_read_finished);
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
        match body_type {
            None => {
                self.http_notes.mark_rsp_no_body();
                clt_send_rsp
                    .send_response(clt_rsp, true)
                    .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
                self.http_notes.rsp_status = self.http_notes.origin_status;
                Ok(true)
            }
            Some(body_type) => {
                let mut clt_send_stream = clt_send_rsp
                    .send_response(clt_rsp, false)
                    .map_err(H2StreamTransferError::ResponseHeadSendFailed)?;
                self.http_notes.rsp_status = self.http_notes.origin_status;
                self.send_h1_origin_body(ups_r, &mut clt_send_stream, body_type)
                    .await?;
                Ok(true)
            }
        }
    }

    async fn send_h1_origin_body(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
        clt_send_stream: &mut SendStream<Bytes>,
        body_type: HttpBodyType,
    ) -> Result<(), H2StreamTransferError> {
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
                                        Some(body_transfer.copied_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(e.into());
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
                                        Some(body_transfer.copied_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(H2StreamTransferError::Idle(
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
                                    Some(body_transfer.copied_size());
                                self.http_notes.clt_rsp_body_size =
                                    Some(body_transfer.copied_size());
                                return Err(H2StreamTransferError::CanceledAsServerQuit);
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
                                        Some(body_transfer.copied_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(e.into());
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
                                        Some(body_transfer.copied_size());
                                    self.http_notes.clt_rsp_body_size =
                                        Some(body_transfer.copied_size());
                                    return Err(H2StreamTransferError::Idle(
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
                                    Some(body_transfer.copied_size());
                                self.http_notes.clt_rsp_body_size =
                                    Some(body_transfer.copied_size());
                                return Err(H2StreamTransferError::CanceledAsServerQuit);
                            }
                        }
                    }
                }
                let n = body_transfer.copied_size();
                drop(body_transfer);
                self.http_notes.mark_rsp_recv_all();
                clt_send_stream.send_data(Bytes::new(), true).map_err(|e| {
                    H2StreamTransferError::ResponseBodyTransferFailed(
                        H2StreamBodyTransferError::SendDataFailed(e),
                    )
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
            origin.connection,
            origin.reuse_notes,
            origin.egress_notes,
        );
    }
}
