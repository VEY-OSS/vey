/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use bytes::BufMut;
use h2::RecvStream;
use http::Response;
use tokio::io::AsyncWrite;
use tokio::time::Instant;

use vey_http::client::HttpAdaptedResponse;
use vey_io_ext::{IdleCheck, StreamCopyConfig};
use vey_types::net::HttpHeaderMap;

use super::H2ToH1RespmodAdaptationError;
use crate::reqmod::h1::HttpRequestForAdaptation;
use crate::respmod::IcapRespmodClient;
use crate::{IcapClientConnection, IcapServiceClient, IcapServiceOptions};

mod bidirectional;
mod client;
mod forward;
mod preview;

use bidirectional::{BidirectionalRecvHttpResponse, BidirectionalRecvIcapResponse};

pub struct H2ToH1ResponseAdapter<I: IdleCheck> {
    icap_client: Arc<IcapServiceClient>,
    icap_connection: IcapClientConnection,
    icap_options: Arc<IcapServiceOptions>,
    copy_config: StreamCopyConfig,
    http_body_line_max_size: usize,
    idle_checker: I,
    client_addr: Option<SocketAddr>,
    client_username: Option<arcstr::ArcStr>,
    tenant_username: Option<arcstr::ArcStr>,
    respond_shared_headers: Option<HttpHeaderMap>,
}

pub struct H2ToH1RespmodRunState {
    task_create_instant: Instant,
    dur_ups_recv_header: Duration,
    pub dur_ups_recv_all: Option<Duration>,
    pub dur_clt_send_header: Option<Duration>,
    pub dur_clt_send_all: Option<Duration>,
    pub ups_read_finished: bool,
    pub clt_write_started: bool,
    pub clt_write_finished: bool,
}

impl H2ToH1RespmodRunState {
    pub fn new(task_create_instant: Instant, dur_ups_recv_header: Duration) -> Self {
        H2ToH1RespmodRunState {
            task_create_instant,
            dur_ups_recv_header,
            dur_ups_recv_all: None,
            dur_clt_send_header: None,
            dur_clt_send_all: None,
            ups_read_finished: false,
            clt_write_started: false,
            clt_write_finished: false,
        }
    }

    pub(crate) fn mark_ups_recv_no_body(&mut self) {
        self.dur_ups_recv_all = Some(self.dur_ups_recv_header);
        self.ups_read_finished = true;
    }

    pub(crate) fn mark_ups_recv_all(&mut self) {
        self.dur_ups_recv_all = Some(self.task_create_instant.elapsed());
        self.ups_read_finished = true;
    }

    pub(crate) fn mark_clt_send_start(&mut self) {
        self.clt_write_started = true;
    }

    pub(crate) fn mark_clt_send_header(&mut self) {
        self.dur_clt_send_header = Some(self.task_create_instant.elapsed());
    }

    pub(crate) fn mark_clt_send_no_body(&mut self) {
        self.dur_clt_send_all = self.dur_clt_send_header;
        self.clt_write_finished = true;
    }

    pub(crate) fn mark_clt_send_all(&mut self) {
        self.dur_clt_send_all = Some(self.task_create_instant.elapsed());
        self.clt_write_finished = true;
    }
}

pub enum H2ToH1RespmodEndState {
    OriginalTransferred,
    AdaptedTransferred(HttpAdaptedResponse),
}

impl IcapRespmodClient {
    pub async fn h2_to_h1_adapter<I: IdleCheck>(
        &self,
        copy_config: StreamCopyConfig,
        http_body_line_max_size: usize,
        idle_checker: I,
    ) -> anyhow::Result<H2ToH1ResponseAdapter<I>> {
        let icap_client = self.inner.clone();
        let (icap_connection, icap_options) = icap_client.fetch_connection().await?;
        Ok(H2ToH1ResponseAdapter {
            icap_client,
            icap_connection,
            icap_options,
            copy_config,
            http_body_line_max_size,
            idle_checker,
            client_addr: None,
            client_username: None,
            tenant_username: None,
            respond_shared_headers: None,
        })
    }
}

impl<I: IdleCheck> H2ToH1ResponseAdapter<I> {
    pub fn set_client_addr(&mut self, addr: SocketAddr) {
        self.client_addr = Some(addr);
    }

    pub fn set_client_username(&mut self, user: arcstr::ArcStr) {
        self.client_username = Some(user);
    }

    pub fn set_tenant_username(&mut self, user: arcstr::ArcStr) {
        self.tenant_username = Some(user);
    }

    pub fn set_respond_shared_headers(&mut self, shared_headers: Option<HttpHeaderMap>) {
        self.respond_shared_headers = shared_headers;
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

    pub async fn xfer<R, CW>(
        self,
        state: &mut H2ToH1RespmodRunState,
        http_request: &R,
        http_response: Response<()>,
        ups_body: RecvStream,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        R: HttpRequestForAdaptation,
        CW: AsyncWrite + Unpin,
    {
        if ups_body.is_end_stream() {
            state.mark_ups_recv_no_body();
            self.xfer_without_body(state, http_request, http_response, clt_writer)
                .await
        } else if let Some(preview_size) = self.preview_size() {
            self.xfer_with_preview(
                state,
                http_request,
                http_response,
                ups_body,
                clt_writer,
                preview_size,
            )
            .await
        } else {
            self.xfer_without_preview(state, http_request, http_response, ups_body, clt_writer)
                .await
        }
    }
}
