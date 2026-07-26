/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use http::{HeaderValue, Version, header};
use tokio::io::AsyncRead;
use tokio::sync::mpsc;
use tokio::time::Instant;

use vey_http::server::{HttpProxyClientRequest, HttpRequestParseError, UriExt};
use vey_http::uri::{HttpMasque, WellKnownUri};
use vey_types::net::{HttpProxySubProtocol, HttpUpgradeToken, UpstreamAddr};

use super::HttpClientReader;
use crate::config::server::http_proxy::HttpProxyServerConfig;

pub(crate) struct HttpProxyRequest<CDR> {
    pub(crate) client_protocol: HttpProxySubProtocol,
    pub(crate) inner: HttpProxyClientRequest,
    pub(crate) upstream: UpstreamAddr,
    pub(crate) time_accepted: Instant,
    pub(crate) time_received: Instant,
    pub(crate) body_reader: Option<HttpClientReader<CDR>>,
    pub(crate) stream_sender: mpsc::Sender<Option<HttpClientReader<CDR>>>,
}

impl<CDR> HttpProxyRequest<CDR>
where
    CDR: AsyncRead + Unpin,
{
    pub(crate) async fn parse(
        config: &HttpProxyServerConfig,
        reader: &mut HttpClientReader<CDR>,
        sender: mpsc::Sender<Option<HttpClientReader<CDR>>>,
        version: &mut Version,
    ) -> Result<(Self, bool), HttpRequestParseError> {
        let time_accepted = Instant::now();

        let mut req = HttpProxyClientRequest::parse(
            reader,
            config.req_hdr_max_size,
            version,
            |req, name, header| {
                match name.as_str() {
                    "proxy-authorization" => return req.parse_header_authorization(header.value),
                    "proxy-connection" => {
                        // proxy-connection is not standard, but at least curl use it
                        return req.parse_header_connection(header);
                    }
                    "forwarded" | "x-forwarded-for" if config.steal_forwarded_for => {
                        return Ok(());
                    }
                    _ => {}
                }
                req.append_parsed_header(name, header)?;
                Ok(())
            },
        )
        .await?;
        let time_received = Instant::now();

        let (upstream, sub_protocol) = if let Some(upgrade_token) = req.upgrade_token() {
            // All upgrade requests are considered a local request here
            let sub_protocol = match upgrade_token {
                HttpUpgradeToken::ConnectUdp => HttpProxySubProtocol::UdpConnect,
                v => return Err(HttpRequestParseError::UnsupportedUpgradeToken(v.clone())),
            };
            match WellKnownUri::parse(&req.uri) {
                Ok(Some(WellKnownUri::Masque(HttpMasque::Udp(addr)))) => (addr, sub_protocol),
                Ok(Some(_v)) => return Err(HttpRequestParseError::UnsupportedUri(req.uri)),
                Ok(None) => return Err(HttpRequestParseError::UnsupportedUri(req.uri)),
                Err(_) => return Err(HttpRequestParseError::UnsupportedUri(req.uri)),
            }
        } else if req.is_connect() {
            let addr = req.uri.get_upstream_with_default_port(443)?;
            (addr, HttpProxySubProtocol::TcpConnect)
        } else if req.is_local_request(&config.local_server_names) {
            match WellKnownUri::parse(&req.uri) {
                Ok(Some(WellKnownUri::EasyProxy(protocol, addr, uri))) => {
                    req.uri = uri;
                    req.set_host(&addr);
                    (addr, protocol)
                }
                Ok(Some(_v)) => return Err(HttpRequestParseError::UnsupportedUri(req.uri)),
                Ok(None) => return Err(HttpRequestParseError::UnsupportedUri(req.uri)),
                Err(_) => return Err(HttpRequestParseError::UnsupportedUri(req.uri)),
            }
        } else {
            req.uri.get_upstream_and_protocol()?
        };

        if !config.allow_custom_host
            && let Some(host) = &req.host
            && !host.host_eq(&upstream)
        {
            return Err(HttpRequestParseError::UnmatchedHostAndAuthority);
        }

        let req = HttpProxyRequest {
            client_protocol: sub_protocol,
            inner: req,
            upstream,
            time_accepted,
            time_received,
            body_reader: None,
            stream_sender: sender,
        };

        let mut send_reader = true;
        match req.client_protocol {
            HttpProxySubProtocol::TcpConnect => {
                // just send to forward task, which will go into a connect task
                // reader should be sent
            }
            HttpProxySubProtocol::UdpConnect => {
                // just send to forward task, which will go into a connect-udp task
                // reader should be sent
            }
            HttpProxySubProtocol::FtpOverHttp => {}
            HttpProxySubProtocol::HttpForward | HttpProxySubProtocol::HttpsForward => {
                if req.inner.pipeline_safe() {
                    // reader should not be sent
                    send_reader = false;
                }
            }
        }

        // reader should be sent by default
        Ok((req, send_reader))
    }

    pub(crate) fn drop_default_port_in_host(&mut self, default_port: u16) {
        if let Some(v) = self.inner.end_to_end_headers.get_mut(header::HOST) {
            let b = v.inner().as_bytes();
            if let Some(new_host) = host_header_without_default_port(b, default_port)
                && let Ok(new_v) = HeaderValue::from_bytes(new_host)
            {
                v.set_inner(new_v);
            }
        }
    }
}

fn host_header_without_default_port(host: &[u8], default_port: u16) -> Option<&[u8]> {
    let colon = if host.starts_with(b"[") {
        let bracket_end = memchr::memchr(b']', host)?;
        if host.get(bracket_end + 1).copied() != Some(b':') {
            return None;
        }
        bracket_end + 1
    } else {
        memchr::memchr(b':', host)?
    };
    let port: u16 = std::str::from_utf8(host.get(colon + 1..)?)
        .ok()?
        .parse()
        .ok()?;
    (port == default_port).then_some(&host[..colon])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_default_port_hostname() {
        assert_eq!(
            host_header_without_default_port(b"example.com:80", 80),
            Some(&b"example.com"[..])
        );
        assert_eq!(
            host_header_without_default_port(b"example.com:8080", 80),
            None
        );
        assert_eq!(host_header_without_default_port(b"example.com", 80), None);
    }

    #[test]
    fn drop_default_port_ipv6() {
        assert_eq!(
            host_header_without_default_port(b"[2001:db8::1]:80", 80),
            Some(&b"[2001:db8::1]"[..])
        );
        assert_eq!(
            host_header_without_default_port(b"[2001:db8::1]:443", 443),
            Some(&b"[2001:db8::1]"[..])
        );
        assert_eq!(
            host_header_without_default_port(b"[2001:db8::1]:8080", 80),
            None
        );
        assert_eq!(host_header_without_default_port(b"[2001:db8::1]", 80), None);
    }
}
