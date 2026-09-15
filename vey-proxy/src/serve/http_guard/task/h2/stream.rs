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

use vey_types::net::{
    Host, HttpForwardedHeaderType, HttpForwardedHeaderValue, HttpUpgradeToken, UpstreamAddr,
};
use vey_types::route::HostMatch;

use super::CommonTaskContext;
use super::error::{H2StreamTransferError, h2_local_error_response};
use super::forward::H2ForwardTask;
use super::websocket::H2WebsocketTask;
use crate::module::http_header::ProxyErrorType;
use crate::serve::http_guard::HttpHost;

pub(super) async fn transfer(
    mut clt_req: Request<RecvStream>,
    mut clt_send_rsp: SendResponse<Bytes>,
    ctx: Arc<CommonTaskContext>,
    hosts: Arc<HostMatch<Arc<HttpHost>>>,
) {
    append_forwarded(clt_req.headers_mut(), &ctx);

    let host = match request_host(&clt_req) {
        Ok(h) => h,
        Err(e) => {
            reply_err(&ctx, &mut clt_send_rsp, &e);
            return;
        }
    };

    let Some(matched) = hosts.get_matched(&host).cloned() else {
        reply_err(
            &ctx,
            &mut clt_send_rsp,
            &H2StreamTransferError::SiteNotFound,
        );
        return;
    };

    if !matched.same_site(ctx.site_ctx.site()) {
        reply_err(
            &ctx,
            &mut clt_send_rsp,
            &H2StreamTransferError::MisdirectedRequest,
        );
        return;
    }

    let site = Arc::clone(ctx.site_ctx.site());

    if clt_req.method().eq(&Method::CONNECT) {
        if let Some(protocol) = clt_req.extensions().get::<Protocol>() {
            let token = HttpUpgradeToken::from_str(protocol.as_str())
                .unwrap_or_else(|_| HttpUpgradeToken::Unsupported(protocol.as_str().to_owned()));
            if !matches!(token, HttpUpgradeToken::Websocket) {
                reply_status(&ctx, &mut clt_send_rsp, StatusCode::NOT_IMPLEMENTED);
                return;
            }
            let task = H2WebsocketTask::new(ctx, site, &clt_req);
            task.run(clt_req, clt_send_rsp).await;
        } else {
            reply_status(&ctx, &mut clt_send_rsp, StatusCode::NOT_IMPLEMENTED);
        }
        return;
    }

    let task = H2ForwardTask::new(ctx, site, &clt_req);
    task.forward(clt_req, clt_send_rsp).await;
}

fn request_host<B>(req: &Request<B>) -> Result<Host, H2StreamTransferError> {
    let authority = match req.uri().authority() {
        Some(auth) => {
            let host = Host::from_str(auth.host())
                .map_err(|_| H2StreamTransferError::InvalidHostHeader)?;
            Some((host, auth.port_u16()))
        }
        None => None,
    };
    let host_header = match req.headers().get(header::HOST) {
        Some(value) => {
            let s = std::str::from_utf8(value.as_bytes())
                .map_err(|_| H2StreamTransferError::InvalidHostHeader)?;
            Some(UpstreamAddr::from_str(s).map_err(|_| H2StreamTransferError::InvalidHostHeader)?)
        }
        None => None,
    };

    if let (Some((auth_host, auth_port)), Some(hdr)) = (&authority, &host_header)
        && !host_matches_authority(auth_host, *auth_port, hdr, req.uri().scheme_str())
    {
        return Err(H2StreamTransferError::UnmatchedHostAndAuthority);
    }

    if let Some((host, _)) = authority {
        return Ok(host);
    }
    if let Some(addr) = host_header {
        return Ok(addr.host().clone());
    }
    Err(H2StreamTransferError::InvalidHostHeader)
}

fn host_matches_authority(
    auth_host: &Host,
    auth_port: Option<u16>,
    hdr: &UpstreamAddr,
    scheme: Option<&str>,
) -> bool {
    if auth_host != hdr.host() {
        return false;
    }
    let default = match scheme {
        Some("http") => Some(80),
        Some("https") => Some(443),
        _ => None,
    };
    normalize_port(auth_port, default) == normalize_port(nonzero_port(hdr.port()), default)
}

fn nonzero_port(port: u16) -> Option<u16> {
    if port == 0 { None } else { Some(port) }
}

fn normalize_port(port: Option<u16>, default: Option<u16>) -> Option<u16> {
    match port {
        Some(p) if default == Some(p) => None,
        other => other,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;

    fn host(uri: &str, host: Option<&str>) -> Result<Host, H2StreamTransferError> {
        let mut b = Request::builder().uri(uri);
        if let Some(h) = host {
            b = b.header(header::HOST, h);
        }
        request_host(&b.body(()).unwrap())
    }

    #[test]
    fn request_host_uses_authority_when_present() {
        assert_eq!(
            host("https://a.example/x", None).unwrap(),
            Host::from_str("a.example").unwrap()
        );
        assert_eq!(
            host("https://a.example/x", Some("a.example")).unwrap(),
            Host::from_str("a.example").unwrap()
        );
        assert_eq!(
            host("https://a.example/x", Some("A.EXAMPLE:443")).unwrap(),
            Host::from_str("a.example").unwrap()
        );
        assert_eq!(
            host("/x", Some("a.example:8443")).unwrap(),
            Host::from_str("a.example").unwrap()
        );
    }

    #[test]
    fn request_host_rejects_unmatched_host_header() {
        assert!(matches!(
            host("https://a.example/x", Some("b.example")),
            Err(H2StreamTransferError::UnmatchedHostAndAuthority)
        ));
        assert!(matches!(
            host("https://a.example/x", Some("a.example:8443")),
            Err(H2StreamTransferError::UnmatchedHostAndAuthority)
        ));
        assert!(matches!(
            host("/x", None),
            Err(H2StreamTransferError::InvalidHostHeader)
        ));
    }
}
