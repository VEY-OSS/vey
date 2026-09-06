/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use futures_util::FutureExt;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_http::client::HttpForwardRemoteResponse;
use vey_icap_client::reqmod::h1::{
    H1ReqmodAdaptationError, HttpAdapterErrorResponse, HttpRequestAdapter,
    ReqmodAdaptationEndState, ReqmodAdaptationRunState, ReqmodRecvHttpResponseBody,
};
use vey_icap_client::respmod::h1::{
    HttpResponseAdapter, RespmodAdaptationEndState, RespmodAdaptationRunState,
};
use vey_io_ext::{LimitedBufReadExt, StreamCopy, StreamCopyError};
use vey_types::net::{HttpHeaderMap, KeepAliveValue};

use super::super::protocol::{HttpClientReader, HttpClientWriter};
use super::HttpGuardForwardTask;
use crate::module::http_forward::BoxHttpForwardConnection;
use crate::serve::{ServerIdleChecker, ServerTaskError, ServerTaskResult, ServerTaskStage};

impl HttpGuardForwardTask<'_> {
    pub(super) async fn run_with_adaptation<CDR, CDW>(
        &mut self,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
        icap_adapter: HttpRequestAdapter<ServerIdleChecker>,
        adaptation_state: &mut ReqmodAdaptationRunState,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        use crate::module::http_forward::HttpForwardWriterForAdaptation;

        let ups_w = &mut ups_c.0;
        let ups_r = &mut ups_c.1;

        let mut ups_w_adaptation = HttpForwardWriterForAdaptation { inner: ups_w };
        let mut adaptation_fut = icap_adapter
            .xfer(
                adaptation_state,
                self.req,
                clt_r.as_mut(),
                &mut ups_w_adaptation,
            )
            .boxed();

        let mut log_interval = self.ctx.get_log_interval();

        let clt_read_size = self.task_stats.clt.read.get_bytes();
        let mut rsp_header: Option<HttpForwardRemoteResponse> = None;
        loop {
            tokio::select! {
                biased;

                r = ups_r.fill_wait_data() => {
                    match r {
                        Ok(true) => {
                            let hdr = self.recv_response_header(ups_r).await?;
                            if let Some(final_hdr) = self.check_out_final_response(hdr, clt_w).await? {
                                rsp_header = Some(final_hdr);
                                break;
                            }
                        }
                        Ok(false) =>  {
                            if self.task_stats.clt.read.get_bytes() == clt_read_size {
                                self.http_notes.retry_new_connection = true;
                            }
                            return Err(ServerTaskError::ClosedByUpstream);
                        },
                        Err(e) => {
                            if self.task_stats.clt.read.get_bytes() == clt_read_size {
                                self.http_notes.retry_new_connection = true;
                            }
                            return Err(ServerTaskError::UpstreamReadFailed(e));
                        },
                    }
                }
                r = &mut adaptation_fut => {
                    match r {
                        Ok(ReqmodAdaptationEndState::OriginalTransferred) => {
                            break;
                        }
                        Ok(ReqmodAdaptationEndState::AdaptedTransferred(_r)) => {
                            break;
                        }
                        Ok(ReqmodAdaptationEndState::HttpErrResponse(rsp, rsp_recv_body)) => {
                            self.send_adaptation_error_response(clt_w, rsp, rsp_recv_body).await?;
                            return Ok(None);
                        }
                        Err(e) => {
                            if self.task_stats.clt.read.get_bytes() == clt_read_size {
                                self.http_notes.retry_new_connection = matches!(
                                    e,
                                    H1ReqmodAdaptationError::IcapServerConnectionClosed | H1ReqmodAdaptationError::IcapServerReadFailed(_)
                                );
                            }
                            return Err(e.into());
                        }
                    }
                }
                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
            }
        }
        drop(adaptation_fut);

        let mut close_remote = false;
        let mut rsp_header = match rsp_header {
            Some(header) => {
                if !adaptation_state.clt_read_finished {
                    self.should_close = true;
                }
                if !adaptation_state.ups_write_finished {
                    close_remote = true;
                }
                header
            }
            None => {
                match tokio::time::timeout(
                    self.rsp_hdr_recv_timeout(),
                    self.recv_final_response_header(ups_r, clt_w),
                )
                .await
                {
                    Ok(Ok(rsp_header)) => rsp_header,
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(ServerTaskError::UpstreamAppTimeout(
                            "timeout to receive response header",
                        ));
                    }
                }
            }
        };
        self.http_notes.mark_rsp_recv_hdr();

        self.send_response(
            clt_w,
            ups_r,
            &mut rsp_header,
            adaptation_state.take_respond_shared_headers(),
        )
        .await?;

        if self.should_relay_websocket() {
            return self.relay_websocket(clt_r, clt_w, ups_c).await;
        }

        self.task_notes.stage = ServerTaskStage::Finished;
        if close_remote {
            let _ = ups_w.shutdown().await;
            Ok(None)
        } else {
            Ok(Some(ups_c))
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
        self.should_close = true;

        self.ctx
            .set_custom_header_for_adaptation_error_reply(&self.egress_notes, &mut rsp);

        let buf = rsp.serialize(self.should_close);
        self.send_error_response = false;
        clt_w
            .write_all(buf.as_ref())
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        self.http_notes.rsp_status = rsp.status.as_u16();

        if let Some(mut recv_body) = rsp_recv_body {
            let mut body_reader = recv_body.body_reader();
            let copy_to_clt =
                StreamCopy::new(&mut body_reader, clt_w, &self.ctx.server_config.tcp_copy);
            copy_to_clt.await.map_err(|e| match e {
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

    pub(super) async fn send_response<R, W>(
        &mut self,
        clt_w: &mut W,
        ups_r: &mut R,
        rsp_header: &mut HttpForwardRemoteResponse,
        adaptation_respond_shared_headers: Option<HttpHeaderMap>,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Send + Unpin,
        W: AsyncWrite + Send + Unpin,
    {
        if self.should_close {
            rsp_header.set_no_keep_alive();
        }
        if !rsp_header.keep_alive() {
            self.should_close = true;
            self.ups_keep_alive = KeepAliveValue::default();
        } else {
            self.ups_keep_alive = rsp_header.keep_alive_header();
            if let Some(notes) = &mut self.alive_reuse_notes {
                notes.overlay_keep_alive(self.ups_keep_alive);
            }
        }
        self.http_notes.origin_status = rsp_header.code;
        self.http_notes.rsp_status = 0;

        if self.audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(respmod) = audit_handle.icap_respmod_client()
        {
            match respmod
                .h1_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.body_line_max_len,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state = RespmodAdaptationRunState::new(
                        self.task_notes.task_created_instant(),
                        self.http_notes.dur_rsp_recv_hdr,
                    );
                    adapter.set_client_addr(self.ctx.client_addr());
                    if let Some(name) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(name.clone());
                    }
                    if let Some(name) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(name.clone());
                    }
                    adapter.set_respond_shared_headers(adaptation_respond_shared_headers);
                    let r = self
                        .send_response_with_adaptation(
                            clt_w,
                            ups_r,
                            rsp_header,
                            adapter,
                            &mut adaptation_state,
                        )
                        .await;
                    if !adaptation_state.clt_write_finished || !adaptation_state.ups_read_finished {
                        self.should_close = true;
                    }
                    if let Some(dur) = adaptation_state.dur_ups_recv_all {
                        self.http_notes.dur_rsp_recv_all = dur;
                    }
                    self.send_error_response = !adaptation_state.clt_write_started;
                    return r;
                }
                Err(e) => {
                    if !respmod.bypass() {
                        return Err(ServerTaskError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.send_response_without_adaptation(clt_w, ups_r, rsp_header)
            .await
    }

    async fn send_response_with_adaptation<R, W>(
        &mut self,
        clt_w: &mut W,
        ups_r: &mut R,
        rsp_header: &HttpForwardRemoteResponse,
        icap_adapter: HttpResponseAdapter<ServerIdleChecker>,
        adaptation_state: &mut RespmodAdaptationRunState,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Send + Unpin,
        W: AsyncWrite + Send + Unpin,
    {
        let mut log_interval = self.ctx.get_log_interval();
        let mut adaptation_fut = icap_adapter
            .xfer(adaptation_state, self.req, rsp_header, ups_r, clt_w)
            .boxed();
        loop {
            tokio::select! {
                biased;

                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
                r = &mut adaptation_fut => {
                    return match r {
                        Ok(RespmodAdaptationEndState::OriginalTransferred) => {
                            self.http_notes.rsp_status = rsp_header.code;
                            Ok(())
                        }
                        Ok(RespmodAdaptationEndState::AdaptedTransferred(adapted_rsp)) => {
                            self.http_notes.rsp_status = adapted_rsp.code;
                            Ok(())
                        }
                        Err(e) => Err(e.into()),
                    }
                }
            }
        }
    }
}
