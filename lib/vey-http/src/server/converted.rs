/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::str::FromStr;

use bytes::BufMut;
use http::{HeaderMap, HeaderValue, Method, Request, Uri, Version, header};

use vey_types::net::{AcceptTransferEncodingValue, UpstreamAddr};

use super::{HttpAdaptedRequest, HttpRequestParseError};
use crate::HttpBodyType;

/// HTTP/2 or HTTP/3 request converted to an HTTP/1.1 hop for origin / ICAP.
///
/// Header names stay lowercase. Hop-by-hop headers that HTTP/2 and HTTP/3
/// do not send (`connection`, and `transfer-encoding` when there is a body)
/// are synthesized on serialize. A body is always announced as chunked so
/// trailers can be forwarded.
pub struct HttpConvertedRequest {
    pub version: Version,
    pub method: Method,
    pub uri: Uri,
    pub end_to_end_headers: HeaderMap,
    pub host: UpstreamAddr,
    chunked: bool,
    trailers: bool,
    expect_100_continue: bool,
    authorization_negotiate: bool,
}

impl HttpConvertedRequest {
    /// Convert an HTTP/2 or HTTP/3 request into an HTTP/1.1 hop.
    ///
    /// Pseudo-headers are already in `method` / `uri`. Header names stay
    /// lowercase. `Host` is taken from the header or URI `:authority`. H2/H3
    /// `te` is kept (typically `trailers`) and listed on `connection`.
    /// `content-length` is dropped.
    pub fn from_request<T>(
        req: &Request<T>,
        has_body: bool,
    ) -> Result<Self, HttpRequestParseError> {
        let mut headers = HeaderMap::new();
        let mut host = None;
        let mut trailers = false;
        let mut expect_100_continue = false;
        let mut authorization_negotiate = false;

        for (name, value) in req.headers() {
            match name.as_str() {
                "connection" | "keep-alive" | "proxy-connection" | "transfer-encoding"
                | "upgrade" | "http2-settings" | "content-length" => continue,
                "te" => {
                    let mut te = AcceptTransferEncodingValue::default();
                    te.parse(value.as_bytes())
                        .map_err(HttpRequestParseError::InvalidAcceptTransferEncoding)?;
                    trailers = te.trailers();
                    continue;
                }
                "host" => {
                    if host.is_some() {
                        return Err(HttpRequestParseError::InvalidHost);
                    }
                    let host_str = value
                        .to_str()
                        .map_err(|_| HttpRequestParseError::InvalidHost)?;
                    if host_str.is_empty() {
                        continue;
                    }
                    host = Some(
                        UpstreamAddr::from_str(host_str)
                            .map_err(|_| HttpRequestParseError::InvalidHost)?,
                    );
                }
                "expect" => {
                    if value.as_bytes().eq_ignore_ascii_case(b"100-continue") {
                        expect_100_continue = true;
                    }
                }
                "authorization" => {
                    if let Ok(s) = value.to_str()
                        && crate::header::is_session_based_auth(s)
                    {
                        authorization_negotiate = true;
                    }
                }
                _ => {}
            }

            headers.append(name, value.clone());
        }

        let host = if let Some(host) = host {
            host
        } else if let Some(authority) = req.uri().authority() {
            let host = UpstreamAddr::from_str(authority.as_str())
                .map_err(|_| HttpRequestParseError::InvalidHost)?;
            let hv = HeaderValue::from_str(authority.as_str())
                .map_err(|_| HttpRequestParseError::InvalidHost)?;
            headers.insert(header::HOST, hv);
            host
        } else {
            return Err(HttpRequestParseError::MissedHost);
        };

        Ok(HttpConvertedRequest {
            version: Version::HTTP_11,
            method: req.method().clone(),
            uri: req.uri().clone(),
            end_to_end_headers: headers,
            host,
            chunked: has_body,
            trailers,
            expect_100_continue,
            authorization_negotiate,
        })
    }

    pub fn adapt_with_body(&self, adapted: HttpAdaptedRequest) -> Self {
        let chunked = adapted.content_length.is_none();
        HttpConvertedRequest {
            version: Version::HTTP_11,
            method: adapted.method,
            uri: adapted.uri,
            end_to_end_headers: HeaderMap::from(adapted.headers),
            host: self.host.clone(),
            chunked,
            trailers: self.trailers,
            expect_100_continue: self.expect_100_continue,
            authorization_negotiate: self.authorization_negotiate,
        }
    }

    pub fn adapt_without_body(&self, adapted: HttpAdaptedRequest) -> Self {
        HttpConvertedRequest {
            version: Version::HTTP_11,
            method: adapted.method,
            uri: adapted.uri,
            end_to_end_headers: HeaderMap::from(adapted.headers),
            host: self.host.clone(),
            chunked: false,
            trailers: false,
            expect_100_continue: self.expect_100_continue,
            authorization_negotiate: self.authorization_negotiate,
        }
    }

    #[inline]
    pub fn keep_alive(&self) -> bool {
        true
    }

    #[inline]
    pub fn expect_100_continue(&self) -> bool {
        self.expect_100_continue
    }

    pub fn body_type(&self) -> Option<HttpBodyType> {
        if self.authorization_negotiate {
            None
        } else if self.chunked {
            Some(HttpBodyType::Chunked)
        } else {
            None
        }
    }

    pub fn serialize_for_origin(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        self.write_request_line(&mut buf);
        write_headers(&self.end_to_end_headers, &mut buf);
        if self.chunked {
            buf.put_slice(b"transfer-encoding: chunked\r\n");
        }
        if self.trailers {
            buf.put_slice(b"te: trailers\r\n");
            buf.put_slice(b"connection: keep-alive, te\r\n");
        } else {
            buf.put_slice(b"connection: keep-alive\r\n");
        }
        buf.put_slice(b"\r\n");
        buf
    }

    pub fn serialize_for_adapter(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);
        self.write_request_line(&mut buf);
        write_headers(&self.end_to_end_headers, &mut buf);
        buf.put_slice(b"\r\n");
        buf
    }

    fn write_request_line(&self, buf: &mut Vec<u8>) {
        if let Some(pa) = self.uri.path_and_query() {
            let _ = write!(buf, "{} {} {:?}\r\n", self.method, pa, self.version);
        } else if self.method == Method::OPTIONS {
            let _ = write!(buf, "OPTIONS * {:?}\r\n", self.version);
        } else {
            let _ = write!(buf, "{} / {:?}\r\n", self.method, self.version);
        }
    }
}

fn write_headers(headers: &HeaderMap, buf: &mut Vec<u8>) {
    for (name, value) in headers {
        buf.put_slice(name.as_ref());
        buf.put_slice(b": ");
        buf.put_slice(value.as_bytes());
        buf.put_slice(b"\r\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_request_synthesizes_connection_and_host() {
        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/v/a")
            .version(Version::HTTP_2)
            .header("accept", "*/*")
            .header("user-agent", "test")
            .body(())
            .unwrap();
        let converted = HttpConvertedRequest::from_request(&req, false).unwrap();
        assert_eq!(converted.version, Version::HTTP_11);
        assert_eq!(converted.method, Method::GET);
        assert!(converted.keep_alive());
        assert!(converted.body_type().is_none());

        let origin = String::from_utf8(converted.serialize_for_origin()).unwrap();
        assert!(origin.starts_with("GET /v/a HTTP/1.1\r\n"));
        assert!(origin.contains("host: example.com\r\n"));
        assert!(origin.contains("connection: keep-alive\r\n"));
        assert!(!origin.contains("transfer-encoding"));
        assert!(origin.contains("accept: */*\r\n"));

        let adapter = String::from_utf8(converted.serialize_for_adapter()).unwrap();
        assert!(!adapter.to_ascii_lowercase().contains("connection:"));
        assert!(!adapter.to_ascii_lowercase().contains("transfer-encoding"));
    }

    #[test]
    fn from_request_with_body_is_chunked_and_drops_content_length() {
        let req = Request::builder()
            .method("POST")
            .uri("https://example.com/upload")
            .version(Version::HTTP_2)
            .header("content-length", "6")
            .header("content-type", "text/plain")
            .body(())
            .unwrap();
        let converted = HttpConvertedRequest::from_request(&req, true).unwrap();
        assert_eq!(converted.body_type(), Some(HttpBodyType::Chunked));

        let origin = String::from_utf8(converted.serialize_for_origin()).unwrap();
        assert!(origin.contains("transfer-encoding: chunked\r\n"));
        assert!(origin.contains("connection: keep-alive\r\n"));
        assert!(!origin.contains("content-length"));
        assert!(origin.contains("content-type: text/plain\r\n"));
    }

    #[test]
    fn from_request_te_trailers_listed_on_connection() {
        let req = Request::builder()
            .method("POST")
            .uri("https://example.com/x")
            .version(Version::HTTP_2)
            .header("te", "trailers")
            .body(())
            .unwrap();
        let converted = HttpConvertedRequest::from_request(&req, true).unwrap();
        let origin = String::from_utf8(converted.serialize_for_origin()).unwrap();
        assert!(origin.contains("te: trailers\r\n"));
        assert!(origin.contains("transfer-encoding: chunked\r\n"));
        assert!(origin.contains("connection: keep-alive, te\r\n"));
    }

    #[test]
    fn from_request_prefers_host_header() {
        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/x")
            .header("host", "other.example:8080")
            .body(())
            .unwrap();
        let converted = HttpConvertedRequest::from_request(&req, false).unwrap();
        let origin = String::from_utf8(converted.serialize_for_origin()).unwrap();
        assert!(origin.contains("host: other.example:8080\r\n"));
        assert!(!origin.contains("host: example.com\r\n"));
    }

    #[test]
    fn from_request_skips_forbidden_hop_by_hop() {
        let req = Request::builder()
            .method("GET")
            .uri("https://example.com/x")
            .header("connection", "close")
            .header("keep-alive", "timeout=5")
            .header("transfer-encoding", "chunked")
            .header("upgrade", "websocket")
            .header("http2-settings", "AAMAAABkAARAAAAAAAIAAAAA")
            .body(())
            .unwrap();
        let converted = HttpConvertedRequest::from_request(&req, false).unwrap();
        assert!(converted.keep_alive());
        assert!(converted.body_type().is_none());

        let origin = String::from_utf8(converted.serialize_for_origin()).unwrap();
        assert!(origin.contains("connection: keep-alive\r\n"));
        assert!(!origin.contains("upgrade:"));
        assert!(!origin.contains("transfer-encoding"));
        assert!(!origin.contains("http2-settings"));
        assert!(!origin.contains("keep-alive:"));
    }

    #[test]
    fn from_request_missed_host() {
        let req = Request::builder().method("GET").uri("/x").body(()).unwrap();
        match HttpConvertedRequest::from_request(&req, false) {
            Err(HttpRequestParseError::MissedHost) => {}
            Err(err) => panic!("unexpected error {err:?}"),
            Ok(_) => panic!("expected MissedHost"),
        }
    }
}
