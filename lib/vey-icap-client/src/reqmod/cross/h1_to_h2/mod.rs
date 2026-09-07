/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arcstr::ArcStr;
use bytes::Bytes;
use h2::RecvStream;
use h2::client::SendRequest;
use http::{Request, Response, StatusCode};
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;

use vey_h2::H2ResponseHeaderReceiver;
use vey_http::server::HttpAdaptedRequest;
use vey_io_ext::{IdleCheck, StreamCopyConfig};
use vey_types::net::HttpHeaderMap;

use super::H1ToH2ReqmodAdaptationError;
use crate::reqmod::IcapReqmodClient;
use crate::reqmod::h1::{
    HttpAdapterErrorResponse, HttpRequestForAdaptation, ReqmodRecvHttpResponseBody,
};
use crate::{IcapClientConnection, IcapServiceClient, IcapServiceOptions};

mod bidirectional;
mod forward;
mod icap;
mod origin;

use bidirectional::{BidirectionalRecvHttpRequest, BidirectionalRecvIcapResponse};

pub struct H1ToH2RequestAdapter<I: IdleCheck> {
    icap_client: Arc<IcapServiceClient>,
    icap_connection: IcapClientConnection,
    icap_options: Arc<IcapServiceOptions>,
    copy_config: StreamCopyConfig,
    http_body_line_max_size: usize,
    http_trailer_max_size: usize,
    http_rsp_head_recv_timeout: Duration,
    http_req_add_no_via_header: bool,
    idle_checker: I,
    client_addr: Option<SocketAddr>,
    client_username: Option<ArcStr>,
    tenant_username: Option<ArcStr>,
    allow_continue: bool,
}

pub struct H1ToH2ReqmodRunState {
    task_create_instant: Instant,
    pub dur_ups_send_header: Option<Duration>,
    pub dur_ups_send_all: Option<Duration>,
    pub dur_ups_recv_header: Option<Duration>,
    pub clt_read_finished: bool,
    pub(crate) respond_shared_headers: Option<HttpHeaderMap>,
}

impl H1ToH2ReqmodRunState {
    pub fn new(task_create_instant: Instant) -> Self {
        H1ToH2ReqmodRunState {
            task_create_instant,
            dur_ups_send_header: None,
            dur_ups_send_all: None,
            dur_ups_recv_header: None,
            clt_read_finished: false,
            respond_shared_headers: None,
        }
    }

    pub fn take_respond_shared_headers(&mut self) -> Option<HttpHeaderMap> {
        self.respond_shared_headers.take()
    }

    pub(crate) fn mark_ups_send_header(&mut self) {
        self.dur_ups_send_header = Some(self.task_create_instant.elapsed());
    }

    pub(crate) fn mark_ups_send_no_body(&mut self) {
        self.dur_ups_send_all = self.dur_ups_send_header;
    }

    pub(crate) fn mark_ups_send_all(&mut self) {
        self.dur_ups_send_all = Some(self.task_create_instant.elapsed());
    }

    pub(crate) fn mark_ups_recv_header(&mut self) {
        self.dur_ups_recv_header = Some(self.task_create_instant.elapsed());
    }
}

pub enum H1ToH2ReqmodEndState {
    OriginalTransferred(Response<RecvStream>),
    AdaptedTransferred(HttpAdaptedRequest, Response<RecvStream>),
    HttpErrResponse(HttpAdapterErrorResponse, Option<ReqmodRecvHttpResponseBody>),
}

impl IcapReqmodClient {
    pub async fn h1_to_h2_adapter<I: IdleCheck>(
        &self,
        copy_config: StreamCopyConfig,
        http_body_line_max_size: usize,
        http_trailer_max_size: usize,
        http_rsp_head_recv_timeout: Duration,
        http_req_add_no_via_header: bool,
        idle_checker: I,
    ) -> anyhow::Result<H1ToH2RequestAdapter<I>> {
        let icap_client = self.inner.clone();
        let (icap_connection, icap_options) = icap_client.fetch_connection().await?;
        Ok(H1ToH2RequestAdapter {
            icap_client,
            icap_connection,
            icap_options,
            copy_config,
            http_body_line_max_size,
            http_trailer_max_size,
            http_rsp_head_recv_timeout,
            http_req_add_no_via_header,
            idle_checker,
            client_addr: None,
            client_username: None,
            tenant_username: None,
            allow_continue: false,
        })
    }
}

impl<I: IdleCheck> H1ToH2RequestAdapter<I> {
    pub fn set_client_addr(&mut self, addr: SocketAddr) {
        self.client_addr = Some(addr);
    }

    pub fn set_client_username(&mut self, user: ArcStr) {
        self.client_username = Some(user);
    }

    pub fn set_tenant_username(&mut self, user: ArcStr) {
        self.tenant_username = Some(user);
    }

    fn push_extended_headers(&self, data: &mut Vec<u8>) {
        if let Some(addr) = self.client_addr {
            crate::serialize::add_client_addr(data, addr);
        }
        if let Some(user) = &self.client_username {
            crate::serialize::add_client_username(data, user);
        }
        if let Some(user) = &self.tenant_username {
            crate::serialize::add_tenant_username(data, user);
        }
    }

    fn preview_size(&self) -> Option<usize> {
        if self.icap_client.config.disable_preview {
            return None;
        }
        self.icap_options.preview_size.filter(|&n| n > 0)
    }

    pub async fn xfer<H, CR, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        http_request: &H,
        clt_body_io: Option<&mut CR>,
        ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        CR: AsyncBufRead + Unpin,
        IW: AsyncWrite + Unpin,
    {
        self.allow_continue = http_request.expect_100_continue();
        if let Some(body_type) = http_request.body_type() {
            let Some(clt_body_io) = clt_body_io else {
                return Err(H1ToH2ReqmodAdaptationError::InternalServerError(
                    "no client http body io supplied while body type is not none",
                ));
            };
            self.xfer_without_preview(
                state,
                http_request,
                body_type,
                clt_body_io,
                ups_send_req,
                clt_informational,
            )
            .await
        } else {
            state.clt_read_finished = true;
            self.xfer_without_body(state, http_request, ups_send_req, clt_informational)
                .await
        }
    }
}

const HTTP_CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";

async fn send_h1_continue<W: AsyncWrite + Unpin>(writer: &mut W) -> io::Result<()> {
    writer.write_all(HTTP_CONTINUE).await?;
    writer.flush().await
}

async fn recv_ups_response_head_after_transfer<IW>(
    ups_recv_rsp: &mut H2ResponseHeaderReceiver,
    clt_informational: &mut IW,
    allow_continue: bool,
    timeout: Duration,
) -> Result<Response<RecvStream>, H1ToH2ReqmodAdaptationError>
where
    IW: AsyncWrite + Unpin,
{
    tokio::time::timeout(
        timeout,
        recv_final_response_after_transfer(ups_recv_rsp, clt_informational, allow_continue),
    )
    .await
    .map_err(|_| H1ToH2ReqmodAdaptationError::HttpUpstreamRecvResponseTimeout)?
}

async fn recv_final_response_after_transfer<IW>(
    ups_recv_rsp: &mut H2ResponseHeaderReceiver,
    clt_informational: &mut IW,
    mut allow_continue: bool,
) -> Result<Response<RecvStream>, H1ToH2ReqmodAdaptationError>
where
    IW: AsyncWrite + Unpin,
{
    loop {
        let rsp = ups_recv_rsp
            .recv_header()
            .await
            .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed)?;
        if let Some(final_rsp) =
            take_final_response(rsp, clt_informational, &mut allow_continue).await?
        {
            return if let Some(body) = ups_recv_rsp.take_body() {
                let (headers, _) = final_rsp.into_parts();
                Ok(Response::from_parts(headers, body))
            } else {
                Err(
                    H1ToH2ReqmodAdaptationError::UnsupportedInformationalResponse(
                        final_rsp.status(),
                    ),
                )
            };
        }
    }
}

async fn take_final_response<IW>(
    rsp: Response<()>,
    clt_informational: &mut IW,
    allow_continue: &mut bool,
) -> Result<Option<Response<()>>, H1ToH2ReqmodAdaptationError>
where
    IW: AsyncWrite + Unpin,
{
    match rsp.status() {
        StatusCode::CONTINUE => {
            if *allow_continue {
                send_h1_continue(clt_informational)
                    .await
                    .map_err(H1ToH2ReqmodAdaptationError::HttpClientWriteFailed)?;
                *allow_continue = false;
            } else {
                return Err(H1ToH2ReqmodAdaptationError::InvalidUpstreamContinueResponse);
            }
        }
        StatusCode::EARLY_HINTS => {
            // H1 has no standard informational framing besides 100; drop 103.
        }
        _ => return Ok(Some(rsp)),
    }
    Ok(None)
}

fn orig_h2_request<H: HttpRequestForAdaptation>(http_request: &H) -> Request<()> {
    http_request.to_h2_request()
}
