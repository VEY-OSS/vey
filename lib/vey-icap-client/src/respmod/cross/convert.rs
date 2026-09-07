/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;

use bytes::BufMut;
use http::{Response, Version, header};

use vey_h2::ResponseExt;
use vey_http::client::HttpAdaptedResponse;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-connection",
    "transfer-encoding",
];

fn skip_h1_hop_by_hop(name: &http::HeaderName) -> bool {
    HOP_BY_HOP.iter().any(|n| name.as_str() == *n) || *name == header::CONTENT_LENGTH
}

/// Serialize an H2 response as HTTP/1.1 for an H1 client.
///
/// Body is always chunked so H2 trailers survive. `Content-Length` is dropped.
pub fn serialize_h2_response_as_h1_chunked(rsp: &Response<()>, has_body: bool) -> Vec<u8> {
    let mut buf = Vec::<u8>::with_capacity(1024);
    let status = rsp.status();
    let reason = status
        .canonical_reason()
        .unwrap_or("NOT STANDARD STATUS CODE");
    let _ = write!(buf, "HTTP/1.1 {} {}\r\n", status.as_u16(), reason);

    for (name, value) in rsp.headers() {
        if skip_h1_hop_by_hop(name) {
            continue;
        }
        buf.put_slice(name.as_ref());
        buf.put_slice(b": ");
        buf.put_slice(value.as_bytes());
        buf.put_slice(b"\r\n");
    }
    if has_body {
        buf.put_slice(b"Transfer-Encoding: chunked\r\n");
    }
    buf.put_slice(b"\r\n");
    buf
}

/// Serialize an ICAP-adapted response as HTTP/1.1 chunked for an H1 client.
///
/// ICAP returns end-to-end headers only. `Content-Length` is dropped so H2
/// trailers can be forwarded as chunked trailers.
pub fn serialize_adapted_response_as_h1_chunked(
    adapted: &HttpAdaptedResponse,
    has_body: bool,
) -> Vec<u8> {
    let mut buf = Vec::<u8>::with_capacity(1024);
    let _ = write!(
        buf,
        "HTTP/1.1 {} {}\r\n",
        adapted.status.as_u16(),
        adapted.reason
    );
    adapted.headers.for_each(|name, value| {
        if *name == header::CONTENT_LENGTH {
            return;
        }
        value.write_to_buf(name, &mut buf);
    });
    if has_body {
        buf.put_slice(b"Transfer-Encoding: chunked\r\n");
    }
    buf.put_slice(b"\r\n");
    buf
}

/// Apply ICAP-adapted headers onto an H2 response.
///
/// ICAP returns end-to-end headers only. Framing is DATA + trailers.
pub fn adapt_response_to_h2(orig: Response<()>, adapted: &HttpAdaptedResponse) -> Response<()> {
    let mut rsp = orig.adapt_to(adapted);
    *rsp.version_mut() = Version::HTTP_2;
    rsp
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;
    use tokio::io::BufReader;
    use vey_http::client::HttpAdaptedResponse;

    #[test]
    fn h2_to_h1_response_uses_chunked_and_drops_content_length() {
        let rsp = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, "4")
            .header("X-Test", "ok")
            .body(())
            .unwrap();
        let text = String::from_utf8(serialize_h2_response_as_h1_chunked(&rsp, true)).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(!text.to_ascii_lowercase().contains("content-length:"));
        assert!(
            text.to_ascii_lowercase()
                .contains("transfer-encoding: chunked\r\n")
        );
        assert!(text.to_ascii_lowercase().contains("x-test: ok\r\n"));
    }

    #[tokio::test]
    async fn adapt_response_to_h2_keeps_http2_version() {
        let mut reader = BufReader::new(&b"HTTP/1.1 204 No Content\r\nX-A: b\r\n\r\n"[..]);
        let adapted = HttpAdaptedResponse::parse(&mut reader, 4096).await.unwrap();
        let orig = Response::builder()
            .status(StatusCode::OK)
            .version(Version::HTTP_2)
            .body(())
            .unwrap();
        let rsp = adapt_response_to_h2(orig, &adapted);
        assert_eq!(rsp.status(), StatusCode::NO_CONTENT);
        assert_eq!(rsp.version(), Version::HTTP_2);
        assert_eq!(rsp.headers().get("x-a").unwrap(), "b");
    }
}
