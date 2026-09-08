/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use h2::RecvStream;
use h2::ext::Protocol;
use h2::server::SendResponse;
use http::{Method, Request, StatusCode, header};

use vey_types::net::{Host, HttpForwardedHeaderType, HttpForwardedHeaderValue, HttpUpgradeToken};
use vey_types::route::HostMatch;

use super::CommonTaskContext;
use super::error::{H2StreamTransferError, h2_local_error_response};
use super::forward::H2ForwardTask;
use super::websocket::H2WebsocketTask;
use crate::config::server::ServerConfig;
use crate::module::http_header::ProxyErrorType;
use crate::serve::ServerStats;
use crate::serve::http_guard::HttpHost;
use crate::site::{SiteContext, SiteHttpConnGuard};

pub(super) async fn transfer(
    mut clt_req: Request<RecvStream>,
    mut clt_send_rsp: SendResponse<Bytes>,
    ctx: Arc<CommonTaskContext>,
    hosts: Arc<HostMatch<Arc<HttpHost>>>,
    site_conn: Arc<Mutex<Option<SiteHttpConnGuard>>>,
) {
    append_forwarded(clt_req.headers_mut(), &ctx);

    let host = match request_host(&clt_req) {
        Ok(h) => h,
        Err(e) => {
            reply_err(&ctx, &mut clt_send_rsp, &e);
            return;
        }
    };

    let Some(matched) = hosts.get(&host).cloned() else {
        reply_err(
            &ctx,
            &mut clt_send_rsp,
            &H2StreamTransferError::SiteNotFound,
        );
        return;
    };

    if let Some(pinned) = &ctx.pinned_site
        && !matched.same_site(pinned)
    {
        reply_err(
            &ctx,
            &mut clt_send_rsp,
            &H2StreamTransferError::MisdirectedRequest,
        );
        return;
    }

    {
        let mut guard = site_conn.lock().unwrap();
        if guard.is_none() {
            *guard = Some(matched.site().hold_http_conn(
                ctx.server_config.name(),
                ctx.server_stats.share_extra_tags(),
            ));
        }
    }

    let site_ctx = SiteContext::new(
        Arc::clone(matched.site()),
        Arc::clone(matched.egress()),
        ctx.server_config.name(),
        ctx.server_stats.share_extra_tags(),
    );

    if clt_req.method().eq(&Method::CONNECT) {
        if let Some(protocol) = clt_req.extensions().get::<Protocol>() {
            let token = HttpUpgradeToken::from_str(protocol.as_str())
                .unwrap_or_else(|_| HttpUpgradeToken::Unsupported(protocol.as_str().to_owned()));
            if !matches!(token, HttpUpgradeToken::Websocket) {
                reply_status(&ctx, &mut clt_send_rsp, StatusCode::NOT_IMPLEMENTED);
                return;
            }
            let task = H2WebsocketTask::new(ctx, site_ctx, Arc::clone(matched.site()), &clt_req);
            task.run(clt_req, clt_send_rsp).await;
        } else {
            reply_status(&ctx, &mut clt_send_rsp, StatusCode::NOT_IMPLEMENTED);
        }
        return;
    }

    let task = H2ForwardTask::new(ctx, site_ctx, Arc::clone(matched.site()), &clt_req);
    task.forward(clt_req, clt_send_rsp).await;
}

fn request_host(req: &Request<RecvStream>) -> Result<Host, H2StreamTransferError> {
    if let Some(auth) = req.uri().authority() {
        return Host::from_str(auth.host()).map_err(|_| H2StreamTransferError::InvalidHostHeader);
    }
    if let Some(value) = req.headers().get(header::HOST) {
        let s = std::str::from_utf8(value.as_bytes())
            .map_err(|_| H2StreamTransferError::InvalidHostHeader)?;
        let host = s.rsplit_once(':').map(|(h, _)| h).unwrap_or(s);
        return Host::from_str(host).map_err(|_| H2StreamTransferError::InvalidHostHeader);
    }
    Err(H2StreamTransferError::InvalidHostHeader)
}

fn append_forwarded(headers: &mut http::HeaderMap, ctx: &CommonTaskContext) {
    match ctx.server_config.append_forwarded_for {
        HttpForwardedHeaderType::Disable => {}
        HttpForwardedHeaderType::Classic => {
            HttpForwardedHeaderValue::new_classic(ctx.client_ip()).append_to_http(headers);
        }
        HttpForwardedHeaderType::Standard => {
            HttpForwardedHeaderValue::new_standard(ctx.client_addr(), ctx.server_addr())
                .append_to_http(headers);
        }
    }
}

fn reply_err(
    ctx: &CommonTaskContext,
    clt_send_rsp: &mut SendResponse<Bytes>,
    e: &H2StreamTransferError,
) {
    if let Some((status, error)) = e.status_and_error()
        && let Some(rsp) = h2_local_error_response(&ctx.server_config, status, error)
    {
        let _ = clt_send_rsp.send_response(rsp, true);
    }
}

fn reply_status(
    ctx: &CommonTaskContext,
    clt_send_rsp: &mut SendResponse<Bytes>,
    status: StatusCode,
) {
    if let Some(rsp) =
        h2_local_error_response(&ctx.server_config, status, ProxyErrorType::HttpRequestError)
    {
        let _ = clt_send_rsp.send_response(rsp, true);
    }
}
