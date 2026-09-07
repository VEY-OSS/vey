/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use http::Request;
use tokio::io::AsyncBufRead;
use tokio::time::Instant;

use vey_http::client::HttpAdaptedResponse;
use vey_io_ext::{IdleCheck, StreamCopyConfig};
use vey_types::net::HttpHeaderMap;

use super::H1ToH2RespmodAdaptationError;
use crate::respmod::IcapRespmodClient;
use crate::respmod::h1::HttpResponseForAdaptation;
use crate::respmod::h2::H2SendResponseToClient;
use crate::{IcapClientConnection, IcapServiceClient, IcapServiceOptions};

mod bidirectional;
mod client;
mod forward;
mod preview;

use bidirectional::{BidirectionalRecvHttpResponse, BidirectionalRecvIcapResponse};

pub struct H1ToH2ResponseAdapter<I: IdleCheck> {
    icap_client: Arc<IcapServiceClient>,
    icap_connection: IcapClientConnection,
    icap_options: Arc<IcapServiceOptions>,
    copy_config: StreamCopyConfig,
    http_body_line_max_size: usize,
    http_trailer_max_size: usize,
    idle_checker: I,
    client_addr: Option<SocketAddr>,
    client_username: Option<String>,
    tenant_username: Option<String>,
    respond_shared_headers: Option<HttpHeaderMap>,
}

pub struct H1ToH2RespmodRunState {
    task_create_instant: Instant,
    dur_ups_recv_header: Duration,
    pub dur_ups_recv_all: Option<Duration>,
    pub dur_clt_send_header: Option<Duration>,
    pub dur_clt_send_all: Option<Duration>,
    pub clt_write_started: bool,
}

impl H1ToH2RespmodRunState {
    pub fn new(task_create_instant: Instant, dur_ups_recv_header: Duration) -> Self {
        H1ToH2RespmodRunState {
            task_create_instant,
            dur_ups_recv_header,
            dur_ups_recv_all: None,
            dur_clt_send_header: None,
            dur_clt_send_all: None,
            clt_write_started: false,
        }
    }

    pub(crate) fn mark_ups_recv_no_body(&mut self) {
        self.dur_ups_recv_all = Some(self.dur_ups_recv_header);
    }

    pub(crate) fn mark_ups_recv_all(&mut self) {
        self.dur_ups_recv_all = Some(self.task_create_instant.elapsed());
    }

    pub(crate) fn mark_clt_send_start(&mut self) {
        self.clt_write_started = true;
    }

    pub(crate) fn mark_clt_send_header(&mut self) {
        self.dur_clt_send_header = Some(self.task_create_instant.elapsed());
    }

    pub(crate) fn mark_clt_send_no_body(&mut self) {
        self.dur_clt_send_all = self.dur_clt_send_header;
    }

    pub(crate) fn mark_clt_send_all(&mut self) {
        self.dur_clt_send_all = Some(self.task_create_instant.elapsed());
    }
}

pub enum H1ToH2RespmodEndState {
    OriginalTransferred,
    AdaptedTransferred(HttpAdaptedResponse),
}

impl IcapRespmodClient {
    pub async fn h1_to_h2_adapter<I: IdleCheck>(
        &self,
        copy_config: StreamCopyConfig,
        http_body_line_max_size: usize,
        http_trailer_max_size: usize,
        idle_checker: I,
    ) -> anyhow::Result<H1ToH2ResponseAdapter<I>> {
        let icap_client = self.inner.clone();
        let (icap_connection, icap_options) = icap_client.fetch_connection().await?;
        Ok(H1ToH2ResponseAdapter {
            icap_client,
            icap_connection,
            icap_options,
            copy_config,
            http_body_line_max_size,
            http_trailer_max_size,
            idle_checker,
            client_addr: None,
            client_username: None,
            tenant_username: None,
            respond_shared_headers: None,
        })
    }
}

impl<I: IdleCheck> H1ToH2ResponseAdapter<I> {
    pub fn set_client_addr(&mut self, addr: SocketAddr) {
        self.client_addr = Some(addr);
    }

    pub fn set_client_username(&mut self, user: &str) {
        self.client_username = Some(user.to_owned());
    }

    pub fn set_tenant_username(&mut self, user: &str) {
        self.tenant_username = Some(user.to_owned());
    }

    pub fn set_respond_shared_headers(&mut self, shared_headers: Option<HttpHeaderMap>) {
        self.respond_shared_headers = shared_headers;
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
        if let Some(map) = &self.respond_shared_headers {
            crate::serialize::add_shared(data, map);
        }
    }

    fn preview_size(&self) -> Option<usize> {
        if self.icap_client.config.disable_preview {
            return None;
        }
        self.icap_options.preview_size.filter(|&n| n > 0)
    }

    pub async fn xfer<H, UR, CW>(
        self,
        state: &mut H1ToH2RespmodRunState,
        http_request: &Request<()>,
        http_response: &H,
        ups_body_io: &mut UR,
        clt_send_response: &mut CW,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        if let Some(body_type) = http_response.body_type(http_request.method()) {
            if let Some(preview_size) = self.preview_size() {
                self.xfer_with_preview(
                    state,
                    http_request,
                    http_response,
                    body_type,
                    ups_body_io,
                    clt_send_response,
                    preview_size,
                )
                .await
            } else {
                self.xfer_without_preview(
                    state,
                    http_request,
                    http_response,
                    body_type,
                    ups_body_io,
                    clt_send_response,
                )
                .await
            }
        } else {
            state.mark_ups_recv_no_body();
            self.xfer_without_body(state, http_request, http_response, clt_send_response)
                .await
        }
    }
}

fn orig_h2_response<H: HttpResponseForAdaptation>(http_response: &H) -> http::Response<()> {
    http_response.to_h2_response()
}
