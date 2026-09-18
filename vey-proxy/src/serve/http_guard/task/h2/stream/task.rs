/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::sync::Arc;

use bytes::Bytes;
use h2::ext::Protocol;
use h2::server::SendResponse;
use h2::{RecvStream, StreamId};
use http::{Method, Request, StatusCode, header};
use uuid::Uuid;

use vey_http::server::UriExt;
use vey_types::net::{HttpUpgradeToken, UpstreamAddr, ViaValue};

use super::{H2ForwardTask, H2TaskContext, H2WebsocketTask};
use crate::log::task::h2_stream::TaskLogForH2Stream;
use crate::module::http_header::ProxyErrorType;
use crate::serve::ServerTaskNotes;

enum StreamOutcome {
    Forward(H2ForwardTask),
    Websocket(H2WebsocketTask),
}

enum H2StreamError {
    InvalidRequestTarget,
    InvalidHostHeader,
    HostMismatch,
    MisdirectedRequest,
    LoopDetected,
    TenantBlocked,
    UnsupportedConnect,
}

impl H2StreamError {
    fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidRequestTarget => "invalid request target",
            Self::InvalidHostHeader => "invalid host header",
            Self::HostMismatch => "host mismatch",
            Self::MisdirectedRequest => "misdirected request",
            Self::LoopDetected => "loop detected",
            Self::TenantBlocked => "tenant blocked",
            Self::UnsupportedConnect => "unsupported connect",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRequestTarget | Self::InvalidHostHeader => StatusCode::BAD_REQUEST,
            Self::HostMismatch => StatusCode::CONFLICT,
            Self::MisdirectedRequest => StatusCode::MISDIRECTED_REQUEST,
            Self::LoopDetected => StatusCode::LOOP_DETECTED,
            Self::TenantBlocked => StatusCode::FORBIDDEN,
            Self::UnsupportedConnect => StatusCode::NOT_IMPLEMENTED,
        }
    }

    fn proxy_error(&self) -> ProxyErrorType {
        match self {
            Self::TenantBlocked => ProxyErrorType::HttpRequestDenied,
            Self::LoopDetected => ProxyErrorType::ProxyLoopDetected,
            _ => ProxyErrorType::HttpRequestError,
        }
    }
}

pub(crate) struct H2StreamTask {
    ctx: Arc<H2TaskContext>,
    clt_stream_id: StreamId,
    task_notes: ServerTaskNotes,
}

impl H2StreamTask {
    pub(crate) fn new(ctx: Arc<H2TaskContext>, clt_stream_id: StreamId) -> Self {
        let task_notes = ServerTaskNotes::new(ctx.cc_info.clone(), None, Default::default())
            .with_site_ctx(ctx.site_ctx.clone());
        H2StreamTask {
            ctx,
            clt_stream_id,
            task_notes,
        }
    }

    fn log_ctx(&self) -> Option<TaskLogForH2Stream<'_>> {
        self.ctx
            .task_logger
            .as_ref()
            .map(|logger| TaskLogForH2Stream {
                logger,
                task_notes: &self.task_notes,
                connection_id: &self.ctx.connection_id,
                clt_stream_id: &self.clt_stream_id,
            })
    }

    fn log(
        &self,
        result: &str,
        next_task_id: Option<&Uuid>,
        next_task_type: Option<&str>,
        rsp_status: Option<u16>,
    ) {
        if let Some(log) = self.log_ctx() {
            log.log(result, next_task_type, next_task_id, rsp_status);
        }
    }

    pub(crate) async fn run(
        self,
        mut clt_req: Request<RecvStream>,
        mut clt_send_rsp: SendResponse<Bytes>,
    ) {
        match self.dispatch(&mut clt_req).await {
            Ok(StreamOutcome::Forward(task)) => {
                self.log("H2Forward", Some(task.task_id()), Some("H2Forward"), None);
                task.forward(clt_req, clt_send_rsp).await;
            }
            Ok(StreamOutcome::Websocket(task)) => {
                self.log("Websocket", Some(task.task_id()), Some("Websocket"), None);
                task.run(clt_req, clt_send_rsp).await;
            }
            Err(e) => {
                self.ctx
                    .reply_early_error(&mut clt_send_rsp, e.status(), e.proxy_error());
                self.log(e.as_str(), None, None, Some(e.status().as_u16()));
            }
        }
    }

    async fn dispatch(
        &self,
        clt_req: &mut Request<RecvStream>,
    ) -> Result<StreamOutcome, H2StreamError> {
        let Some(upstream) = clt_req
            .uri()
            .get_optional_http_https_upstream()
            .ok()
            .flatten()
        else {
            return Err(H2StreamError::InvalidRequestTarget);
        };
        if let Some(value) = clt_req.headers().get(header::HOST) {
            let parsed = std::str::from_utf8(value.as_bytes())
                .ok()
                .and_then(|s| UpstreamAddr::from_str(s).ok());
            let Some(mut host) = parsed else {
                return Err(H2StreamError::InvalidHostHeader);
            };
            if host.port() == 0 {
                host.set_port(upstream.port());
            }
            if !host.host_eq(&upstream) || host.port() != upstream.port() {
                return Err(H2StreamError::HostMismatch);
            }
        }

        if !self.ctx.site_ctx.site().covers_host(upstream.host()) {
            return Err(H2StreamError::MisdirectedRequest);
        }

        let via = ViaValue::for_hop(
            clt_req.version(),
            self.ctx.server_config.server_id.as_ref(),
            upstream.host_str(),
        );
        if via.seen_in_http(clt_req.headers()) {
            return Err(H2StreamError::LoopDetected);
        }
        via.append_to_http(clt_req.headers_mut());

        if let Some(delay) = self.ctx.site_ctx.tenant_user_blocked_delay() {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            return Err(H2StreamError::TenantBlocked);
        }

        if clt_req.method().eq(&Method::CONNECT) {
            if let Some(protocol) = clt_req.extensions().get::<Protocol>() {
                let token = HttpUpgradeToken::from_str(protocol.as_str()).unwrap_or_else(|_| {
                    HttpUpgradeToken::Unsupported(protocol.as_str().to_owned())
                });
                if !matches!(token, HttpUpgradeToken::Websocket) {
                    return Err(H2StreamError::UnsupportedConnect);
                }
                self.ctx.append_forwarded(clt_req);
                return Ok(StreamOutcome::Websocket(H2WebsocketTask::new(
                    Arc::clone(&self.ctx),
                    self.clt_stream_id,
                    clt_req,
                )));
            }
            return Err(H2StreamError::UnsupportedConnect);
        }

        self.ctx.append_forwarded(clt_req);
        Ok(StreamOutcome::Forward(H2ForwardTask::new(
            Arc::clone(&self.ctx),
            self.clt_stream_id,
            clt_req,
        )))
    }
}
