/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;

/// RFC 9209 proxy error types used on locally generated error responses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProxyErrorType {
    ConnectionTimeout,
    DnsTimeout,
    DnsError,
    DestinationIpProhibited,
    DestinationIpUnroutable,
    ConnectionRefused,
    ConnectionTerminated,
    TlsProtocolError,
    HttpRequestError,
    HttpRequestDenied,
    HttpResponseTimeout,
    HttpProtocolError,
    DestinationUnavailable,
    ProxyInternalError,
    ProxyLoopDetected,
}

impl ProxyErrorType {
    pub(crate) fn as_token(self) -> &'static str {
        match self {
            Self::ConnectionTimeout => "connection_timeout",
            Self::DnsTimeout => "dns_timeout",
            Self::DnsError => "dns_error",
            Self::DestinationIpProhibited => "destination_ip_prohibited",
            Self::DestinationIpUnroutable => "destination_ip_unroutable",
            Self::ConnectionRefused => "connection_refused",
            Self::ConnectionTerminated => "connection_terminated",
            Self::TlsProtocolError => "tls_protocol_error",
            Self::HttpRequestError => "http_request_error",
            Self::HttpRequestDenied => "http_request_denied",
            Self::HttpResponseTimeout => "http_response_timeout",
            Self::HttpProtocolError => "http_protocol_error",
            Self::DestinationUnavailable => "destination_unavailable",
            Self::ProxyInternalError => "proxy_internal_error",
            Self::ProxyLoopDetected => "proxy_loop_detected",
        }
    }
}

pub(crate) const DEFAULT_PROXY_STATUS_IDENT: &str = "vey-proxy";

/// RFC 9651 sf-token: ( ALPHA / "*" ) *( tchar / ":" / "/" )
fn is_sf_token(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some('A'..='Z' | 'a'..='z' | '*') => {}
        _ => return false,
    }
    chars.all(|c| {
        matches!(
            c,
            'A'..='Z'
                | 'a'..='z'
                | '0'..='9'
                | '!'
                | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
                | ':'
                | '/'
        )
    })
}

fn write_sf_ident(buf: &mut Vec<u8>, ident: &str) {
    if is_sf_token(ident) {
        buf.extend_from_slice(ident.as_bytes());
        return;
    }

    buf.push(b'"');
    for b in ident.as_bytes() {
        if *b == b'\\' || *b == b'"' {
            buf.push(b'\\');
        }
        buf.push(*b);
    }
    buf.push(b'"');
}

/// Format the RFC 9209 `Proxy-Status` field value (no header name or CRLF).
pub(crate) fn proxy_status_value(ident: &str, error: ProxyErrorType) -> String {
    let mut buf = Vec::with_capacity(16 + ident.len());
    write_sf_ident(&mut buf, ident);
    let _ = write!(buf, "; error={}", error.as_token());
    unsafe { String::from_utf8_unchecked(buf) }
}

/// Format a locally generated `Proxy-Status` header line, including the trailing CRLF.
pub(crate) fn proxy_status(ident: &str, error: ProxyErrorType) -> String {
    format!("Proxy-Status: {}\r\n", proxy_status_value(ident, error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_ident() {
        assert_eq!(
            proxy_status("vey-proxy", ProxyErrorType::DnsError),
            "Proxy-Status: vey-proxy; error=dns_error\r\n"
        );
        assert_eq!(
            proxy_status("edge.example.net", ProxyErrorType::ConnectionTimeout),
            "Proxy-Status: edge.example.net; error=connection_timeout\r\n"
        );
    }

    #[test]
    fn quoted_ident() {
        assert_eq!(
            proxy_status("edge 1", ProxyErrorType::HttpRequestDenied),
            "Proxy-Status: \"edge 1\"; error=http_request_denied\r\n"
        );
        assert_eq!(
            proxy_status("say\"hi", ProxyErrorType::DnsError),
            "Proxy-Status: \"say\\\"hi\"; error=dns_error\r\n"
        );
        assert_eq!(
            proxy_status("123edge", ProxyErrorType::ConnectionRefused),
            "Proxy-Status: \"123edge\"; error=connection_refused\r\n"
        );
    }
}
