/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, Write};

use tokio::io::{AsyncWrite, AsyncWriteExt};

use vey_types::net::UpstreamAddr;

/// the extra header lines should end with \r\n
pub struct HttpConnectRequest<'a> {
    static_headers: &'a [String],
    dyn_headers: Vec<String>,
}

impl<'a> HttpConnectRequest<'a> {
    pub fn new(static_headers: &'a [String]) -> Self {
        HttpConnectRequest {
            static_headers,
            dyn_headers: Vec::new(),
        }
    }

    pub fn append_dyn_header(&mut self, line: String) {
        debug_assert!(line.ends_with("\r\n"));
        self.dyn_headers.push(line);
    }

    pub async fn send<W>(&'a self, target: &UpstreamAddr, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buf = Vec::with_capacity(256);
        write!(&mut buf, "CONNECT {target} HTTP/1.1\r\n")?;
        buf.extend_from_slice(b"Connection: keep-alive\r\n");
        for line in self.static_headers {
            debug_assert!(line.ends_with("\r\n"));
            buf.extend_from_slice(line.as_bytes());
        }
        for line in &self.dyn_headers {
            buf.extend_from_slice(line.as_bytes());
        }
        buf.extend_from_slice(b"\r\n");
        writer.write_all(&buf).await?;
        writer.flush().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_upstream_addr() -> UpstreamAddr {
        UpstreamAddr::from_str("example.com:8080").unwrap()
    }

    async fn render_request(request: &HttpConnectRequest<'_>, target: &UpstreamAddr) -> String {
        let mut buffer = Vec::new();
        request.send(target, &mut buffer).await.unwrap();
        String::from_utf8(buffer).unwrap()
    }

    #[test]
    fn new_starts_with_static_headers_only() {
        let static_headers = vec![
            "User-Agent: test-agent\r\n".to_string(),
            "Accept: */*\r\n".to_string(),
        ];

        let request = HttpConnectRequest::new(&static_headers);

        assert_eq!(request.static_headers.len(), 2);
        assert!(request.dyn_headers.is_empty());
    }

    #[test]
    fn append_dyn_header_preserves_order() {
        let static_headers: Vec<String> = Vec::new();
        let mut request = HttpConnectRequest::new(&static_headers);

        request.append_dyn_header("X-Custom-Header: value1\r\n".to_string());
        request.append_dyn_header("X-Another-Header: value2\r\n".to_string());

        assert_eq!(request.dyn_headers.len(), 2);
        assert_eq!(request.dyn_headers[0], "X-Custom-Header: value1\r\n");
        assert_eq!(request.dyn_headers[1], "X-Another-Header: value2\r\n");
    }

    #[tokio::test]
    async fn send_without_extra_headers() {
        let target = test_upstream_addr();
        let static_headers: Vec<String> = Vec::new();
        let request = HttpConnectRequest::new(&static_headers);

        assert_eq!(
            render_request(&request, &target).await,
            "CONNECT example.com:8080 HTTP/1.1\r\n\
             Connection: keep-alive\r\n\
             \r\n"
        );
    }

    #[tokio::test]
    async fn send_with_static_and_dynamic_headers() {
        let target = test_upstream_addr();
        let static_headers = vec![
            "User-Agent: test-agent\r\n".to_string(),
            "Accept: */*\r\n".to_string(),
        ];
        let mut request = HttpConnectRequest::new(&static_headers);

        request.append_dyn_header("X-Custom-Header: value1\r\n".to_string());
        request.append_dyn_header("X-Another-Header: value2\r\n".to_string());

        assert_eq!(
            render_request(&request, &target).await,
            "CONNECT example.com:8080 HTTP/1.1\r\n\
             Connection: keep-alive\r\n\
             User-Agent: test-agent\r\n\
             Accept: */*\r\n\
             X-Custom-Header: value1\r\n\
             X-Another-Header: value2\r\n\
             \r\n"
        );
    }

    #[tokio::test]
    async fn send_with_ipv6_host() {
        let target = UpstreamAddr::from_str("[2001:db8::1]:8080").unwrap();
        let static_headers: Vec<String> = Vec::new();
        let request = HttpConnectRequest::new(&static_headers);

        assert_eq!(
            render_request(&request, &target).await,
            "CONNECT [2001:db8::1]:8080 HTTP/1.1\r\n\
             Connection: keep-alive\r\n\
             \r\n"
        );
    }

    #[tokio::test]
    async fn send_propagates_writer_error() {
        let target = test_upstream_addr();
        let static_headers: Vec<String> = Vec::new();
        let request = HttpConnectRequest::new(&static_headers);
        let mut writer = tokio_test::io::Builder::new()
            .write_error(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"))
            .build();

        let err = request.send(&target, &mut writer).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
