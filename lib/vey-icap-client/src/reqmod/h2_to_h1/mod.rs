/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arcstr::ArcStr;
use bytes::BufMut;
use h2::RecvStream;
use http::HeaderMap;
use tokio::time::Instant;

use vey_io_ext::{IdleCheck, StreamCopyConfig};

use super::IcapReqmodClient;
pub use crate::reqmod::h1::{
    HttpAdapterErrorResponse, HttpRequestForAdaptation, HttpRequestUpstreamWriter,
};
pub use crate::reqmod::h2::ReqmodRecvHttpResponseBody;
use crate::{IcapClientConnection, IcapServiceClient, IcapServiceOptions};

mod error;
pub use error::H2ToH1ReqmodAdaptationError;

mod bidirectional;
use bidirectional::{BidirectionalRecvHttpRequest, BidirectionalRecvIcapResponse};

mod recv_request;
mod recv_response;

mod forward_body;
mod forward_header;
mod preview;

impl IcapReqmodClient {
    pub async fn h2_to_h1_adapter<I: IdleCheck>(
        &self,
        copy_config: StreamCopyConfig,
        http_body_line_max_size: usize,
        http_trailer_max_size: usize,
        http_req_add_no_via_header: bool,
        idle_checker: I,
    ) -> anyhow::Result<H2ToH1RequestAdapter<I>> {
        let icap_client = self.inner.clone();
        let (icap_connection, icap_options) = icap_client.fetch_connection().await?;
        Ok(H2ToH1RequestAdapter {
            icap_client,
            icap_connection,
            icap_options,
            copy_config,
            http_body_line_max_size,
            http_trailer_max_size,
            http_req_add_no_via_header,
            idle_checker,
            client_addr: None,
            client_username: None,
            tenant_username: None,
        })
    }
}

pub struct H2ToH1RequestAdapter<I: IdleCheck> {
    icap_client: Arc<IcapServiceClient>,
    icap_connection: IcapClientConnection,
    icap_options: Arc<IcapServiceOptions>,
    copy_config: StreamCopyConfig,
    http_body_line_max_size: usize,
    http_trailer_max_size: usize,
    http_req_add_no_via_header: bool,
    idle_checker: I,
    client_addr: Option<SocketAddr>,
    client_username: Option<ArcStr>,
    tenant_username: Option<ArcStr>,
}

pub struct ReqmodAdaptationRunState {
    task_create_instant: Instant,
    pub dur_ups_send_header: Option<Duration>,
    pub dur_ups_send_all: Option<Duration>,
    pub clt_read_finished: bool,
    pub ups_write_finished: bool,
    pub clt_req_body_size: Option<u64>,
    pub ups_req_body_size: Option<u64>,
    pub(crate) respond_shared_headers: Option<HeaderMap>,
}

impl ReqmodAdaptationRunState {
    pub fn new(task_create_instant: Instant) -> Self {
        ReqmodAdaptationRunState {
            task_create_instant,
            dur_ups_send_header: None,
            dur_ups_send_all: None,
            clt_read_finished: false,
            ups_write_finished: false,
            clt_req_body_size: None,
            ups_req_body_size: None,
            respond_shared_headers: None,
        }
    }

    pub fn take_respond_shared_headers(&mut self) -> Option<HeaderMap> {
        self.respond_shared_headers.take()
    }

    pub(crate) fn mark_ups_send_header(&mut self) {
        self.dur_ups_send_header = Some(self.task_create_instant.elapsed());
    }

    pub(crate) fn mark_ups_send_no_body(&mut self) {
        self.dur_ups_send_all = self.dur_ups_send_header;
        self.ups_write_finished = true;
        self.ups_req_body_size = Some(0);
    }

    pub(crate) fn mark_ups_send_all(&mut self) {
        self.dur_ups_send_all = Some(self.task_create_instant.elapsed());
        self.ups_write_finished = true;
    }
}

impl<I: IdleCheck> H2ToH1RequestAdapter<I> {
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
        data.put_slice(b"X-Transformed-From: HTTP/2.0\r\n");
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

    pub async fn xfer<H, UW>(
        self,
        state: &mut ReqmodAdaptationRunState,
        http_request: &H,
        clt_body: RecvStream,
        ups_writer: &mut UW,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        if clt_body.is_end_stream() {
            state.clt_read_finished = true;
            state.clt_req_body_size = Some(0);
            self.xfer_without_body(state, http_request, ups_writer)
                .await
        } else if let Some(preview_size) = self.preview_size() {
            self.xfer_with_preview(state, http_request, clt_body, ups_writer, preview_size)
                .await
        } else {
            self.xfer_without_preview(state, http_request, clt_body, ups_writer)
                .await
        }
    }
}

pub enum ReqmodAdaptationEndState<H: HttpRequestForAdaptation> {
    OriginalTransferred,
    AdaptedTransferred(H),
    HttpErrResponse(HttpAdapterErrorResponse, Option<ReqmodRecvHttpResponseBody>),
}

pub enum ReqmodAdaptationMidState<H: HttpRequestForAdaptation> {
    OriginalRequest,
    AdaptedRequest(H),
    HttpErrResponse(HttpAdapterErrorResponse, Option<ReqmodRecvHttpResponseBody>),
}
