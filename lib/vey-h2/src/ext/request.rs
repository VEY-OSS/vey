/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::str::FromStr;

use bytes::BufMut;
use http::uri::Authority;
use http::{HeaderMap, Method, Request, Uri, header};

use vey_http::server::HttpAdaptedRequest;
use vey_types::net::{Host, UpstreamAddr};

pub trait RequestExt {
    fn serialize_for_adapter(&self) -> Vec<u8>;
    fn adapt_to(self, other: &HttpAdaptedRequest) -> Self;
    fn clone_header(&self) -> Request<()>;
    fn expect_100_continue(&self) -> bool;
    fn authorization_negotiate(&self) -> bool;
    fn maybe_grpc(&self) -> bool;
    fn host(&self) -> Host;
}

impl<T> RequestExt for Request<T> {
    fn serialize_for_adapter(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::with_capacity(1024);
        let method = self.method();
        let uri = self.uri();
        if let Some(pa) = uri.path_and_query() {
            let _ = write!(buf, "{method} {pa} HTTP/1.1\r\n");
        } else if method.eq(&Method::OPTIONS) {
            buf.extend_from_slice(b"OPTIONS * HTTP/1.1\r\n");
        } else {
            let _ = write!(buf, "{method} / HTTP/1.1\r\n");
        }
        for (name, value) in self.headers() {
            if matches!(name, &header::TE) {
                // skip hop-by-hop headers
                continue;
            }
            buf.put_slice(name.as_ref());
            buf.put_slice(b": ");
            buf.put_slice(value.as_bytes());
            buf.put_slice(b"\r\n");
        }
        if !self.headers().contains_key(header::HOST)
            && let Some(host) = uri.host()
        {
            buf.put_slice(b"Host: ");
            buf.put_slice(host.as_bytes());
            buf.put_slice(b"\r\n");
        }
        buf.put_slice(b"\r\n");
        buf
    }

    fn adapt_to(self, other: &HttpAdaptedRequest) -> Self {
        let mut headers = HeaderMap::from(&other.headers);
        vey_http::header::remove_h2_connection_specific_headers(&mut headers);
        // add hop-by-hop headers
        if let Some(v) = self.headers().get(header::TE) {
            headers.insert(header::TE, v.into());
        }
        let (mut parts, body) = self.into_parts();
        parts.method = other.method.clone();
        let mut uri_parts = other.uri.clone().into_parts();
        uri_parts.scheme = parts.uri.scheme().cloned();
        uri_parts.authority = parts.uri.authority().cloned();
        if let Some(host) = headers.remove(header::HOST) {
            // we should always remove the Host header to be compatible with Google,
            // but let's keep the same as client behaviour here
            if parts.headers.contains_key(header::HOST) {
                headers.insert(header::HOST, host.clone());
            }
            if uri_parts.authority.is_none()
                && let Ok(authority) = Authority::from_maybe_shared(host.clone())
            {
                //update the authority field
                uri_parts.authority = Some(authority);
            }
        }
        if let Ok(new_uri) = Uri::from_parts(uri_parts) {
            parts.uri = new_uri;
        }
        // keep old version
        parts.headers = headers;
        Request::from_parts(parts, body)
    }

    fn clone_header(&self) -> Request<()> {
        let (mut parts, _) = Request::new(()).into_parts();
        parts.method = self.method().clone();
        parts.uri = self.uri().clone();
        parts.version = self.version();
        parts.headers = self.headers().clone();
        Request::from_parts(parts, ())
    }

    fn expect_100_continue(&self) -> bool {
        for v in self.headers().get_all(header::EXPECT) {
            if v.as_bytes() == b"100-continue" {
                return true;
            }
        }

        false
    }

    fn authorization_negotiate(&self) -> bool {
        self.headers()
            .get_all(header::AUTHORIZATION)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(vey_http::header::is_session_based_auth)
    }

    fn maybe_grpc(&self) -> bool {
        self.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(maybe_grpc_content_type)
    }

    fn host(&self) -> Host {
        if let Some(value) = self.headers().get(header::HOST)
            && let Ok(s) = std::str::from_utf8(value.as_bytes())
            && let Ok(addr) = UpstreamAddr::from_str(s)
        {
            return addr.host().clone();
        }
        self.uri()
            .host()
            .and_then(|h| Host::from_str(h).ok())
            .unwrap_or_else(Host::empty)
    }
}

fn maybe_grpc_content_type(value: &str) -> bool {
    const PREFIX: &[u8] = b"application/grpc";
    let v = value.trim_ascii_start().as_bytes();
    if v.len() < PREFIX.len() || !v[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return false;
    }
    matches!(v.get(PREFIX.len()), None | Some(b'+' | b';' | b' ' | b'\t'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Version;

    #[test]
    fn serialize_for_adapter_includes_method_and_path() {
        let req = Request::builder()
            .method(Method::GET)
            .uri("http://example.com/foo?bar=1")
            .header("X-Test", "value")
            .body(())
            .unwrap();

        let serialized = req.serialize_for_adapter();
        let text = String::from_utf8(serialized).unwrap();

        assert!(text.starts_with("GET /foo?bar=1 HTTP/1.1\r\n"));
        assert!(text.to_ascii_lowercase().contains("x-test: value\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn serialize_for_adapter_options_star() {
        let req = Request::builder()
            .method(Method::OPTIONS)
            .uri("*")
            .body(())
            .unwrap();

        let serialized = req.serialize_for_adapter();
        assert_eq!(
            String::from_utf8(serialized).unwrap(),
            "OPTIONS * HTTP/1.1\r\n\r\n"
        );
    }

    #[test]
    fn serialize_for_adapter_skips_te_and_injects_host() {
        let req = Request::builder()
            .method(Method::POST)
            .uri("http://example.com/submit")
            .header(header::TE, "trailers")
            .header("X-Keep", "yes")
            .body(())
            .unwrap();

        let text = String::from_utf8(req.serialize_for_adapter()).unwrap();

        assert!(!text.contains("te:"));
        assert!(!text.contains("TE:"));
        assert!(text.to_ascii_lowercase().contains("x-keep: yes\r\n"));
        assert!(text.to_ascii_lowercase().contains("host: example.com\r\n"));
    }

    #[test]
    fn expect_100_continue_detects_header() {
        let req = Request::builder()
            .header(header::EXPECT, "100-continue")
            .body(())
            .unwrap();
        assert!(req.expect_100_continue());

        let req = Request::builder()
            .header(header::EXPECT, "other")
            .body(())
            .unwrap();
        assert!(!req.expect_100_continue());
    }

    #[test]
    fn authorization_negotiate_detects_session_auth() {
        let req = Request::builder()
            .header(header::AUTHORIZATION, "Negotiate abc")
            .body(())
            .unwrap();
        assert!(req.authorization_negotiate());

        let req = Request::builder()
            .header(header::AUTHORIZATION, "NTLM TlRMTVNTUA==")
            .body(())
            .unwrap();
        assert!(req.authorization_negotiate());

        let req = Request::builder()
            .header(header::AUTHORIZATION, "Basic abc")
            .body(())
            .unwrap();
        assert!(!req.authorization_negotiate());
    }

    #[test]
    fn maybe_grpc_detects_content_type() {
        let req = Request::builder()
            .header(header::CONTENT_TYPE, "application/grpc")
            .body(())
            .unwrap();
        assert!(req.maybe_grpc());

        let req = Request::builder()
            .header(header::CONTENT_TYPE, "application/grpc+proto")
            .body(())
            .unwrap();
        assert!(req.maybe_grpc());

        let req = Request::builder()
            .header(header::CONTENT_TYPE, "APPLICATION/GRPC; charset=utf-8")
            .body(())
            .unwrap();
        assert!(req.maybe_grpc());

        let req = Request::builder()
            .header(header::CONTENT_TYPE, "application/grpc-web+proto")
            .body(())
            .unwrap();
        assert!(!req.maybe_grpc());

        let req = Request::builder()
            .header(header::CONTENT_TYPE, "application/json")
            .body(())
            .unwrap();
        assert!(!req.maybe_grpc());
    }

    #[test]
    fn host_prefers_host_header_then_uri() {
        let req = Request::builder()
            .uri("https://uri.example/x")
            .header(header::HOST, "header.example:8080")
            .body(())
            .unwrap();
        assert_eq!(req.host(), Host::from_str("header.example").unwrap());

        let req = Request::builder()
            .uri("https://uri.example/x")
            .body(())
            .unwrap();
        assert_eq!(req.host(), Host::from_str("uri.example").unwrap());

        let req = Request::builder().uri("/x").body(()).unwrap();
        assert!(req.host().is_empty());
    }

    #[test]
    fn clone_header_copies_parts_without_body() {
        let req = Request::builder()
            .method(Method::PUT)
            .uri("https://example.com/a")
            .version(Version::HTTP_2)
            .header("X-Test", "1")
            .body("body")
            .unwrap();

        let header_only = req.clone_header();
        assert_eq!(header_only.method(), Method::PUT);
        assert_eq!(header_only.uri(), req.uri());
        assert_eq!(header_only.version(), Version::HTTP_2);
        assert_eq!(header_only.headers().get("X-Test").unwrap(), "1");
    }

    #[test]
    fn serialize_for_adapter_default_path_without_query() {
        let req = Request::builder()
            .method(Method::GET)
            .uri("http://example.com")
            .body(())
            .unwrap();

        let text = String::from_utf8(req.serialize_for_adapter()).unwrap();
        assert!(text.starts_with("GET / HTTP/1.1\r\n"));
        assert!(text.to_ascii_lowercase().contains("host: example.com\r\n"));
    }

    #[tokio::test]
    async fn adapt_to_replaces_method_and_uri() {
        use tokio::io::BufReader;
        use vey_http::server::HttpAdaptedRequest;

        let mut reader = BufReader::new(&b"POST /new HTTP/1.1\r\n\r\n"[..]);
        let adapted = HttpAdaptedRequest::parse(&mut reader, 4096, false)
            .await
            .unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("https://old.example/path?x=1")
            .header(header::HOST, "old.example")
            .header(header::TE, "trailers")
            .body("body")
            .unwrap();

        let adapted_req = req.adapt_to(&adapted);
        assert_eq!(adapted_req.method(), Method::POST);
        assert_eq!(adapted_req.uri().path(), "/new");
        assert_eq!(adapted_req.uri().scheme().unwrap().as_str(), "https");
        assert_eq!(adapted_req.headers().get(header::TE).unwrap(), "trailers");
    }

    #[tokio::test]
    async fn adapt_to_drops_connection_specific_headers() {
        use tokio::io::BufReader;
        use vey_http::server::HttpAdaptedRequest;

        let mut reader = BufReader::new(
            &b"GET /new HTTP/1.1\r\nX-New: 1\r\nUpgrade: h2c\r\nProxy-Connection: close\r\n\r\n"[..],
        );
        let adapted = HttpAdaptedRequest::parse(&mut reader, 4096, false)
            .await
            .unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("https://old.example/path")
            .body(())
            .unwrap();

        let adapted_req = req.adapt_to(&adapted);
        assert_eq!(adapted_req.headers().len(), 1);
        assert_eq!(adapted_req.headers().get("X-New").unwrap(), "1");
    }
}
