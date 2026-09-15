/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::sync::Arc;

use bytes::Bytes;
use h2::RecvStream;
use h2::ext::Protocol;
use h2::server::SendResponse;
use http::{Method, Request, StatusCode, header};

use vey_http::server::UriExt;
use vey_types::net::{HttpUpgradeToken, UpstreamAddr};
use vey_types::route::HostMatch;

use super::H2TaskContext;
use super::forward::H2ForwardTask;
use super::websocket::H2WebsocketTask;
use crate::module::http_header::ProxyErrorType;
use crate::serve::http_guard::HttpHost;

pub(super) async fn transfer(
    mut clt_req: Request<RecvStream>,
    mut clt_send_rsp: SendResponse<Bytes>,
    ctx: Arc<H2TaskContext>,
    hosts: Arc<HostMatch<Arc<HttpHost>>>,
) {
    let Some(upstream) = clt_req
        .uri()
        .get_optional_http_https_upstream()
        .ok()
        .flatten()
    else {
        ctx.reply_early_error(
            &mut clt_send_rsp,
            StatusCode::BAD_REQUEST,
            ProxyErrorType::HttpRequestError,
        );
        return;
    };
    if let Some(value) = clt_req.headers().get(header::HOST) {
        let parsed = std::str::from_utf8(value.as_bytes())
            .ok()
            .and_then(|s| UpstreamAddr::from_str(s).ok());
        let Some(mut host) = parsed else {
            ctx.reply_early_error(
                &mut clt_send_rsp,
                StatusCode::BAD_REQUEST,
                ProxyErrorType::HttpRequestError,
            );
            return;
        };
        if host.port() == 0 {
            host.set_port(upstream.port());
        }
        if !host.host_eq(&upstream) || host.port() != upstream.port() {
            ctx.reply_early_error(
                &mut clt_send_rsp,
                StatusCode::CONFLICT,
                ProxyErrorType::HttpRequestError,
            );
            return;
        }
    }

    let Some(matched) = hosts.get_matched(upstream.host()).cloned() else {
        ctx.reply_early_error(
            &mut clt_send_rsp,
            StatusCode::BAD_REQUEST,
            ProxyErrorType::HttpRequestError,
        );
        return;
    };

    if !matched.same_site(ctx.site_ctx.site()) {
        ctx.reply_early_error(
            &mut clt_send_rsp,
            StatusCode::MISDIRECTED_REQUEST,
            ProxyErrorType::HttpRequestError,
        );
        return;
    }

    if clt_req.method().eq(&Method::CONNECT) {
        if let Some(protocol) = clt_req.extensions().get::<Protocol>() {
            let token = HttpUpgradeToken::from_str(protocol.as_str())
                .unwrap_or_else(|_| HttpUpgradeToken::Unsupported(protocol.as_str().to_owned()));
            if !matches!(token, HttpUpgradeToken::Websocket) {
                ctx.reply_early_error(
                    &mut clt_send_rsp,
                    StatusCode::NOT_IMPLEMENTED,
                    ProxyErrorType::HttpRequestError,
                );
                return;
            }
            ctx.append_forwarded(clt_req.headers_mut());
            let task = H2WebsocketTask::new(ctx, clt_send_rsp.stream_id(), &clt_req);
            task.run(clt_req, clt_send_rsp).await;
        } else {
            ctx.reply_early_error(
                &mut clt_send_rsp,
                StatusCode::NOT_IMPLEMENTED,
                ProxyErrorType::HttpRequestError,
            );
        }
        return;
    }

    ctx.append_forwarded(clt_req.headers_mut());
    let task = H2ForwardTask::new(ctx, clt_send_rsp.stream_id(), &clt_req);
    task.forward(clt_req, clt_send_rsp).await;
}
