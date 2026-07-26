/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;

use bytes::BufMut;
use http::{HeaderMap, Response};

use vey_http::client::HttpAdaptedResponse;

pub trait ResponseExt {
    fn serialize_for_adapter(&self) -> Vec<u8>;
    fn adapt_to(self, other: &HttpAdaptedResponse) -> Self;
}

impl<T> ResponseExt for Response<T> {
    fn serialize_for_adapter(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::with_capacity(1024);

        let status = self.status();
        let reason = self
            .status()
            .canonical_reason()
            .unwrap_or("NOT STANDARD STATUS CODE");
        let _ = write!(buf, "HTTP/1.1 {} {}\r\n", status.as_u16(), reason);

        for (name, value) in self.headers() {
            buf.put_slice(name.as_ref());
            buf.put_slice(b": ");
            buf.put_slice(value.as_bytes());
            buf.put_slice(b"\r\n");
        }
        buf.put_slice(b"\r\n");
        buf
    }

    fn adapt_to(self, other: &HttpAdaptedResponse) -> Self {
        let (mut parts, body) = self.into_parts();
        // keep old version
        parts.status = other.status;
        parts.headers = HeaderMap::from(&other.headers);
        Response::from_parts(parts, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    #[test]
    fn serialize_for_adapter_includes_status_and_headers() {
        let rsp = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("X-Test", "missing")
            .body(())
            .unwrap();

        let text = String::from_utf8(rsp.serialize_for_adapter()).unwrap();

        assert!(text.starts_with("HTTP/1.1 404 Not Found\r\n"));
        assert!(text.to_ascii_lowercase().contains("x-test: missing\r\n"));
        assert!(text.ends_with("\r\n\r\n"));
    }
}
