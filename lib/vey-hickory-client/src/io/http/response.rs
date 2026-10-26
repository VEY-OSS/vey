/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use bytes::{Buf, BufMut, BytesMut};
use hickory_net::NetError;
use hickory_proto::op::DnsResponse;
use http::{Response, header};

/// Upper bound from the allocation comment: a response body must not grow past this.
const MAX_RESPONSE_BODY: usize = 4096;

pub struct HttpDnsResponse {
    rsp: Response<()>,
    content_length: Option<usize>,
    body: BytesMut,
}

impl HttpDnsResponse {
    pub fn new(rsp: Response<()>) -> Result<Self, NetError> {
        let headers = rsp.headers();

        if let Some(ct) = headers.get(header::CONTENT_TYPE)
            && ct.as_bytes() != super::MIME_APPLICATION_DNS.as_bytes()
        {
            return Err(NetError::Msg(format!(
                "unsupported ContentType, should be {}",
                super::MIME_APPLICATION_DNS
            )));
        }

        let content_length = if let Some(cl) = headers.get(header::CONTENT_LENGTH) {
            let s = cl
                .to_str()
                .map_err(|e| NetError::Msg(format!("invalid Content-Length header: {e}")))?;
            let len = usize::from_str(s)
                .map_err(|e| NetError::Msg(format!("invalid Content-Length header: {e}")))?;
            Some(len)
        } else {
            None
        };
        if let Some(len) = content_length
            && len > MAX_RESPONSE_BODY
        {
            return Err(NetError::Msg(format!(
                "Content-Length {len} exceeds the {MAX_RESPONSE_BODY} byte limit"
            )));
        }

        let response_bytes =
            BytesMut::with_capacity(content_length.unwrap_or(512).clamp(512, MAX_RESPONSE_BODY));

        Ok(HttpDnsResponse {
            rsp,
            content_length,
            body: response_bytes,
        })
    }

    pub fn push_body<T: Buf>(&mut self, buf: T) -> Result<(), NetError> {
        let limit = self
            .content_length
            .unwrap_or(MAX_RESPONSE_BODY)
            .min(MAX_RESPONSE_BODY);
        let next = self.body.len().saturating_add(buf.remaining());
        if next > limit {
            return Err(NetError::Msg(format!(
                "response body of {next} bytes exceeds the {limit} byte limit"
            )));
        }
        self.body.put(buf);
        Ok(())
    }

    pub fn body_end(&self) -> bool {
        if let Some(content_length) = self.content_length
            && self.body.len() >= content_length
        {
            return true;
        }
        false
    }

    pub fn into_dns_response(self) -> Result<DnsResponse, NetError> {
        // assert the length
        if let Some(content_length) = self.content_length
            && self.body.len() != content_length
        {
            // TODO: make explicit error type
            return Err(NetError::Msg(format!(
                "expected byte length: {}, got: {}",
                content_length,
                self.body.len()
            )));
        }

        // Was it a successful request?
        if !self.rsp.status().is_success() {
            let error_string = String::from_utf8_lossy(self.body.as_ref());

            // TODO: make explicit error type
            return Err(NetError::Msg(format!(
                "http unsuccessful code: {}, message: {}",
                self.rsp.status(),
                error_string
            )));
        }

        // and finally convert the bytes into a DNS message
        DnsResponse::from_buffer(self.body.to_vec()).map_err(NetError::Proto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Response;
    use http::StatusCode;
    use http::header;

    const MIME_APPLICATION_DNS: &str = "application/dns-message";

    fn dns_response(headers: http::HeaderMap) -> Response<()> {
        let mut rsp = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, MIME_APPLICATION_DNS)
            .body(())
            .unwrap();
        *rsp.headers_mut() = headers;
        rsp
    }

    #[test]
    fn new_rejects_unsupported_content_type() {
        let rsp = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(())
            .unwrap();

        match HttpDnsResponse::new(rsp) {
            Err(e) => assert!(e.to_string().contains("unsupported ContentType")),
            Ok(_) => panic!("expected unsupported content type error"),
        }
    }

    #[test]
    fn body_end_respects_content_length() {
        let mut headers = http::HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, http::HeaderValue::from_static("6"));
        let mut rsp = HttpDnsResponse::new(dns_response(headers)).unwrap();
        assert!(!rsp.body_end());

        rsp.push_body(&b"abc"[..]).unwrap();
        assert!(!rsp.body_end());

        rsp.push_body(&b"def"[..]).unwrap();
        assert!(rsp.body_end());
    }

    #[test]
    fn into_dns_response_rejects_length_mismatch() {
        let mut headers = http::HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, http::HeaderValue::from_static("4"));
        let mut rsp = HttpDnsResponse::new(dns_response(headers)).unwrap();
        rsp.push_body(&b"abc"[..]).unwrap();

        match rsp.into_dns_response() {
            Err(e) => assert!(e.to_string().contains("expected byte length: 4, got: 3")),
            Ok(_) => panic!("expected length mismatch error"),
        }
    }

    #[test]
    fn new_rejects_invalid_content_length() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            header::CONTENT_LENGTH,
            http::HeaderValue::from_static("abc"),
        );
        let rsp = dns_response(headers);
        match HttpDnsResponse::new(rsp) {
            Err(e) => assert!(e.to_string().contains("invalid Content-Length header")),
            Ok(_) => panic!("expected invalid Content-Length error"),
        }
    }

    #[test]
    fn into_dns_response_rejects_non_success_status() {
        let mut rsp = Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .header(header::CONTENT_TYPE, MIME_APPLICATION_DNS)
            .body(())
            .unwrap();
        rsp.headers_mut()
            .insert(header::CONTENT_LENGTH, http::HeaderValue::from_static("5"));
        let mut http_rsp = HttpDnsResponse::new(rsp).unwrap();
        http_rsp.push_body(&b"error"[..]).unwrap();

        match http_rsp.into_dns_response() {
            Err(e) => assert!(e.to_string().contains("http unsuccessful code: 502")),
            Ok(_) => panic!("expected HTTP error"),
        }
    }

    #[test]
    fn body_end_without_content_length_never_true() {
        let rsp = HttpDnsResponse::new(dns_response(http::HeaderMap::new())).unwrap();
        assert!(!rsp.body_end());
    }

    #[test]
    fn new_rejects_content_length_above_the_body_cap() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            header::CONTENT_LENGTH,
            http::HeaderValue::from_static("10000000"),
        );
        match HttpDnsResponse::new(dns_response(headers)) {
            Err(e) => assert!(e.to_string().contains("4096")),
            Ok(_) => panic!("expected body cap error"),
        }
    }

    #[test]
    fn push_body_stops_at_the_cap_without_content_length() {
        let mut rsp = HttpDnsResponse::new(dns_response(http::HeaderMap::new())).unwrap();
        rsp.push_body(&[0u8; 4096][..]).unwrap();
        match rsp.push_body(&b"x"[..]) {
            Err(e) => assert!(e.to_string().contains("4096")),
            Ok(_) => panic!("expected body cap error"),
        }
    }

    #[test]
    fn push_body_stops_at_content_length() {
        let mut headers = http::HeaderMap::new();
        headers.insert(header::CONTENT_LENGTH, http::HeaderValue::from_static("4"));
        let mut rsp = HttpDnsResponse::new(dns_response(headers)).unwrap();
        match rsp.push_body(&b"abcde"[..]) {
            Err(e) => assert!(e.to_string().contains("exceeds the 4 byte limit")),
            Ok(_) => panic!("expected content-length cap error"),
        }
    }
}
