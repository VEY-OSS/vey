/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;

use bytes::BufMut;
use http::uri::Authority;
use http::{HeaderMap, Method, Request, Uri, Version, header};

use vey_h2::RequestExt;
use vey_http::server::HttpAdaptedRequest;
use vey_types::net::HttpHeaderMap;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "transfer-encoding",
    "te",
    "upgrade",
];

fn skip_h1_hop_by_hop(name: &http::HeaderName) -> bool {
    HOP_BY_HOP.iter().any(|n| name.as_str() == *n) || *name == header::CONTENT_LENGTH
}

/// Serialize an H2 request as HTTP/1.1 for an H1 origin.
///
/// Body is always announced as chunked so H2 trailers can be forwarded.
/// `Content-Length` is dropped: a fixed-length H1 body cannot carry trailers.
pub fn serialize_h2_request_as_h1_chunked(req: &Request<()>, has_body: bool) -> Vec<u8> {
    let mut buf = Vec::<u8>::with_capacity(1024);
    let method = req.method();
    let uri = req.uri();
    if let Some(pa) = uri.path_and_query() {
        let _ = write!(buf, "{method} {pa} HTTP/1.1\r\n");
    } else if method.eq(&Method::OPTIONS) {
        buf.extend_from_slice(b"OPTIONS * HTTP/1.1\r\n");
    } else {
        let _ = write!(buf, "{method} / HTTP/1.1\r\n");
    }
    for (name, value) in req.headers() {
        if skip_h1_hop_by_hop(name) {
            continue;
        }
        buf.put_slice(name.as_ref());
        buf.put_slice(b": ");
        buf.put_slice(value.as_bytes());
        buf.put_slice(b"\r\n");
    }
    if !req.headers().contains_key(header::HOST)
        && let Some(host) = uri.host()
    {
        buf.put_slice(b"Host: ");
        buf.put_slice(host.as_bytes());
        buf.put_slice(b"\r\n");
    }
    if has_body {
        buf.put_slice(b"Transfer-Encoding: chunked\r\n");
    }
    buf.put_slice(b"\r\n");
    buf
}

/// Serialize an ICAP-adapted request as HTTP/1.1 chunked for an H1 origin.
///
/// ICAP returns end-to-end headers only. Adapted `Content-Length` is ignored
/// so ICAP/H2 trailers are not dropped.
pub fn serialize_adapted_request_as_h1_chunked(
    adapted: &HttpAdaptedRequest,
    orig: &Request<()>,
    has_body: bool,
) -> Vec<u8> {
    let mut buf = Vec::<u8>::with_capacity(1024);
    let method = &adapted.method;
    if let Some(pa) = adapted.uri.path_and_query() {
        let _ = write!(buf, "{method} {pa} HTTP/1.1\r\n");
    } else if method.eq(&Method::OPTIONS) {
        buf.extend_from_slice(b"OPTIONS * HTTP/1.1\r\n");
    } else {
        let _ = write!(buf, "{method} / HTTP/1.1\r\n");
    }
    adapted.headers.for_each(|name, value| {
        if *name == header::CONTENT_LENGTH {
            return;
        }
        value.write_to_buf(name, &mut buf);
    });
    if !adapted.headers.contains_key(header::HOST) {
        if let Some(v) = orig.headers().get(header::HOST) {
            buf.put_slice(b"Host: ");
            buf.put_slice(v.as_bytes());
            buf.put_slice(b"\r\n");
        } else if let Some(host) = orig.uri().host() {
            buf.put_slice(b"Host: ");
            buf.put_slice(host.as_bytes());
            buf.put_slice(b"\r\n");
        }
    }
    if has_body {
        buf.put_slice(b"Transfer-Encoding: chunked\r\n");
    }
    buf.put_slice(b"\r\n");
    buf
}

/// Apply ICAP-adapted headers onto an H2 request.
///
/// ICAP returns end-to-end headers only. `Content-Length` is kept as metadata;
/// framing is always DATA + optional trailers.
pub fn adapt_request_to_h2(orig: Request<()>, adapted: &HttpAdaptedRequest) -> Request<()> {
    let mut req = orig.adapt_to(adapted);
    *req.version_mut() = Version::HTTP_2;
    if req.uri().authority().is_none()
        && let Some(host) = req.headers().get(header::HOST)
        && let Ok(authority) = Authority::from_maybe_shared(host.clone())
    {
        let mut parts = req.uri().clone().into_parts();
        parts.authority = Some(authority);
        if let Ok(uri) = Uri::from_parts(parts) {
            *req.uri_mut() = uri;
        }
    }
    req
}

pub fn adapted_headers_as_map(headers: &HttpHeaderMap) -> HeaderMap {
    HeaderMap::from(headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header;
    use tokio::io::BufReader;
    use vey_http::server::HttpAdaptedRequest;

    #[test]
    fn h2_to_h1_drops_content_length_and_uses_chunked() {
        let req = Request::builder()
            .method(Method::POST)
            .uri("http://example.com/upload")
            .header(header::CONTENT_LENGTH, "12")
            .header("X-Test", "1")
            .body(())
            .unwrap();

        let text = String::from_utf8(serialize_h2_request_as_h1_chunked(&req, true)).unwrap();
        assert!(text.starts_with("POST /upload HTTP/1.1\r\n"));
        assert!(!text.to_ascii_lowercase().contains("content-length:"));
        assert!(
            text.to_ascii_lowercase()
                .contains("transfer-encoding: chunked\r\n")
        );
        assert!(text.to_ascii_lowercase().contains("x-test: 1\r\n"));
        assert!(text.to_ascii_lowercase().contains("host: example.com\r\n"));
    }

    #[test]
    fn h2_to_h1_without_body_omits_transfer_encoding() {
        let req = Request::builder()
            .method(Method::GET)
            .uri("http://example.com/")
            .body(())
            .unwrap();
        let text = String::from_utf8(serialize_h2_request_as_h1_chunked(&req, false)).unwrap();
        assert!(!text.to_ascii_lowercase().contains("transfer-encoding:"));
        assert!(!text.to_ascii_lowercase().contains("content-length:"));
    }

    #[tokio::test]
    async fn adapt_request_to_h2_strips_transfer_encoding() {
        let mut reader = BufReader::new(
            &b"POST /new HTTP/1.1\r\nTransfer-Encoding: chunked\r\nX-A: b\r\n\r\n"[..],
        );
        let adapted = HttpAdaptedRequest::parse(&mut reader, 4096, false)
            .await
            .unwrap();

        let orig = Request::builder()
            .method(Method::GET)
            .uri("https://old.example/path")
            .version(Version::HTTP_2)
            .body(())
            .unwrap();
        let req = adapt_request_to_h2(orig, &adapted);
        assert_eq!(req.method(), Method::POST);
        assert_eq!(req.version(), Version::HTTP_2);
        assert!(req.headers().get(header::TRANSFER_ENCODING).is_none());
        assert_eq!(req.headers().get("x-a").unwrap(), "b");
    }
}
