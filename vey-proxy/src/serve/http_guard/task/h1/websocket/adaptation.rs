/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use vey_http::client::HttpForwardRemoteResponse;
use vey_http::server::HttpProxyClientRequest;
use vey_icap_client::reqmod::IcapReqmodClient;
use vey_icap_client::reqmod::h1::{
    HttpAdapterErrorResponse, HttpRequestAdapter, ReqmodAdaptationMidState,
    ReqmodAdaptationRunState, ReqmodRecvHttpResponseBody,
};
use vey_io_ext::{StreamCopy, StreamCopyError};

use super::super::protocol::HttpClientWriter;
use super::HttpGuardWebsocketTask;
use crate::module::tcp_connect::TcpConnection;
use crate::serve::{ServerIdleChecker, ServerTaskError, ServerTaskResult};

impl HttpGuardWebsocketTask {
    pub(super) async fn run_icap<CDW>(
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
}
