/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use http::Version;
use tokio::io::AsyncRead;
use tokio::sync::mpsc;
use tokio::time::Instant;

use vey_http::server::{HttpProxyClientRequest, HttpRequestParseError, UriExt};
use vey_types::net::{HttpServerId, HttpUpgradeToken, UpstreamAddr, ViaValue};

use super::HttpClientReader;

pub(crate) struct HttpGuardRequest<CDR> {
    pub(crate) inner: HttpProxyClientRequest,
    pub(crate) upstream: UpstreamAddr,
    pub(crate) time_accepted: Instant,
    pub(crate) time_received: Instant,
    pub(crate) body_reader: Option<HttpClientReader<CDR>>,
    pub(crate) stream_sender: mpsc::Sender<Option<HttpClientReader<CDR>>>,
}

impl<CDR> HttpGuardRequest<CDR>
where
    CDR: AsyncRead + Unpin,
{
    pub(crate) async fn parse(
        reader: &mut HttpClientReader<CDR>,
        sender: mpsc::Sender<Option<HttpClientReader<CDR>>>,
        max_header_size: usize,
        server_id: Option<&HttpServerId>,
        version: &mut Version,
    ) -> Result<(Self, bool), HttpRequestParseError> {
        let time_accepted = Instant::now();

        let mut req =
            HttpProxyClientRequest::parse(reader, max_header_size, version, |req, name, header| {
                if name.as_str() == "authorization" {
                    return req.parse_header_authorization(header.value);
                }
                req.append_parsed_header(name, header)?;
                Ok(())
            })
            .await?;
        let time_received = Instant::now();

        if req.is_connect() {
            return Err(HttpRequestParseError::UnsupportedMethod("CONNECT".into()));
        }
        if let Some(v) = req.upgrade_token()
            && !matches!(v, HttpUpgradeToken::Websocket)
        {
            return Err(HttpRequestParseError::UnsupportedUpgradeToken(v.clone()));
        }

        let upstream = if let Some(mut host) = req.host.clone() {
            if let Some(u) = req.uri.get_optional_http_https_upstream()? {
                if !host.host_eq(&u) {
                    return Err(HttpRequestParseError::UnmatchedHostAndAuthority);
                }
                if host.port() == 0 {
                    host.set_port(u.port());
                }
            }
            host
        } else {
            return Err(HttpRequestParseError::MissedHost);
        };

        // check VIA
        let via = ViaValue::for_hop(req.version, server_id, upstream.host_str());
        if via.seen_in_h1(&req.end_to_end_headers) {
            return Err(HttpRequestParseError::LoopDetected);
        }
        via.append_to_h1(&mut req.end_to_end_headers);

        let req = HttpGuardRequest {
            inner: req,
            upstream,
            time_accepted,
            time_received,
            body_reader: None,
            stream_sender: sender,
        };

        let send_reader = !req.inner.pipeline_safe() || req.inner.upgrade_token().is_some();
        Ok((req, send_reader))
    }
}
