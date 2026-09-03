/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::str::FromStr;

use bytes::{BufMut, Bytes, BytesMut};
use http::{HeaderName, Method, Version, header};
use tokio::io::AsyncBufRead;

use vey_io_ext::LimitedBufReadExt;
use vey_types::net::http_names;
use vey_types::net::{
    ConnectionValue, HttpHeaderMap, HttpHeaderValue, HttpKnownHeaderName, HttpUpgradeToken,
    KeepAliveValue, TransferEncodingValue,
};

use super::{HttpAdaptedResponse, HttpResponseParseError};
use crate::{HttpBodyType, HttpHeaderLine, HttpLineParseError, HttpStatusLine};

pub struct HttpTransparentResponse {
    pub version: Version,
    pub code: u16,
    pub reason: String,
    pub end_to_end_headers: HttpHeaderMap,
    pub hop_by_hop_headers: HttpHeaderMap,
    original_connection_name: HttpKnownHeaderName<http_names::CONNECTION>,
    connection: ConnectionValue,
    origin_header_size: usize,
    keep_alive: bool,
    pub upgrade: Option<HttpUpgradeToken>,
    content_length: u64,
    transfer_encoding: TransferEncodingValue,
    original_transfer_encoding_name: HttpKnownHeaderName<http_names::TRANSFER_ENCODING>,
    has_content_length: bool,
}

impl HttpTransparentResponse {
    fn new(version: Version, code: u16, reason: String) -> Self {
        HttpTransparentResponse {
            version,
            code,
            reason,
            end_to_end_headers: HttpHeaderMap::default(),
            hop_by_hop_headers: HttpHeaderMap::default(),
            original_connection_name: HttpKnownHeaderName::new(),
            connection: ConnectionValue::default(),
            origin_header_size: 0,
            keep_alive: false,
            upgrade: None,
            content_length: 0,
            transfer_encoding: TransferEncodingValue::default(),
            original_transfer_encoding_name: HttpKnownHeaderName::new(),
            has_content_length: false,
        }
    }

    pub fn adapt_with_body(&self, adapted: HttpAdaptedResponse) -> Self {
        let hop_by_hop_headers = self.hop_by_hop_headers.clone();
        match adapted.content_length {
            Some(content_length) => HttpTransparentResponse {
                version: adapted.version,
                code: adapted.status.as_u16(),
                reason: adapted.reason,
                end_to_end_headers: adapted.headers,
                hop_by_hop_headers,
                original_connection_name: self.original_connection_name,
                connection: self.connection.clone(),
                origin_header_size: self.origin_header_size,
                keep_alive: self.keep_alive,
                upgrade: self.upgrade.clone(),
                content_length,
                transfer_encoding: TransferEncodingValue::default(),
                original_transfer_encoding_name: self.original_transfer_encoding_name.cleared(),
                has_content_length: true,
            },
            None => HttpTransparentResponse {
                version: adapted.version,
                code: adapted.status.as_u16(),
                reason: adapted.reason,
                end_to_end_headers: adapted.headers,
                hop_by_hop_headers,
                original_connection_name: self.original_connection_name,
                connection: self.connection.clone(),
                origin_header_size: self.origin_header_size,
                keep_alive: self.keep_alive,
                upgrade: self.upgrade.clone(),
                content_length: 0,
                transfer_encoding: if self.transfer_encoding.chunked() {
                    self.transfer_encoding
                } else {
                    TransferEncodingValue::CHUNKED
                },
                original_transfer_encoding_name: self
                    .original_transfer_encoding_name
                    .received_or_default(),
                has_content_length: false,
            },
        }
    }

    pub fn adapt_without_body(&self, adapted: HttpAdaptedResponse) -> Self {
        let hop_by_hop_headers = self.hop_by_hop_headers.clone();
        let mut end_to_end_headers = adapted.headers;
        if let Some(mut v) = end_to_end_headers.remove(header::CONTENT_LENGTH) {
            v.set_static_value("0");
            end_to_end_headers.insert(header::CONTENT_LENGTH, v);
        } else {
            end_to_end_headers.insert(header::CONTENT_LENGTH, HttpHeaderValue::from_static("0"));
        }
        HttpTransparentResponse {
            version: adapted.version,
            code: adapted.status.as_u16(),
            reason: adapted.reason,
            end_to_end_headers,
            hop_by_hop_headers,
            original_connection_name: self.original_connection_name,
            connection: self.connection.clone(),
            origin_header_size: self.origin_header_size,
            keep_alive: self.keep_alive,
            upgrade: self.upgrade.clone(),
            content_length: 0,
            transfer_encoding: TransferEncodingValue::default(),
            original_transfer_encoding_name: self.original_transfer_encoding_name.cleared(),
            has_content_length: true,
        }
    }

    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }

    #[inline]
    pub fn keep_alive_header(&self) -> KeepAliveValue {
        self.connection.keep_alive_header()
    }

    pub fn set_no_keep_alive(&mut self) {
        self.connection.clear_keep_alive_header();
        self.keep_alive = false;
    }

    fn expect_no_body(&self, method: &Method) -> bool {
        self.code < 200 || self.code == 204 || self.code == 304 || method.eq(&Method::HEAD)
    }

    pub fn body_type(&self, method: &Method) -> Option<HttpBodyType> {
        // see https://tools.ietf.org/html/rfc7230#section-3.3.1 for the Transfer-Encoding
        // see https://tools.ietf.org/html/rfc7230#section-3.3.2 for the Content-Length
        // see https://datatracker.ietf.org/doc/html/rfc7230#section-3.3.3 for Message Body Length
        if self.expect_no_body(method) {
            None
        } else if self.transfer_encoding.chunked() {
            Some(HttpBodyType::Chunked)
        } else if self.original_transfer_encoding_name.is_received() {
            Some(HttpBodyType::ReadUntilEnd)
        } else if self.has_content_length {
            if self.content_length > 0 {
                Some(HttpBodyType::ContentLength(self.content_length))
            } else {
                None
            }
        } else {
            Some(HttpBodyType::ReadUntilEnd)
        }
    }

    pub async fn parse<R>(
        reader: &mut R,
        method: &Method,
        keep_alive: bool,
        max_header_size: usize,
    ) -> Result<(Self, Bytes), HttpResponseParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut head_bytes = BytesMut::with_capacity(4096);

        let (found, nr) = reader
            .limited_read_buf_until(b'\n', max_header_size, &mut head_bytes)
            .await?;
        if nr == 0 {
            return Err(HttpResponseParseError::RemoteClosed);
        }
        if !found {
            return if nr < max_header_size {
                Err(HttpResponseParseError::RemoteClosed)
            } else {
                Err(HttpResponseParseError::TooLargeHeader(max_header_size))
            };
        }

        let mut rsp = HttpTransparentResponse::build_from_status_line(head_bytes.as_ref())?;

        loop {
            let header_size = head_bytes.len();
            if header_size >= max_header_size {
                return Err(HttpResponseParseError::TooLargeHeader(max_header_size));
            }
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_buf_until(b'\n', max_len, &mut head_bytes)
                .await?;
            if nr == 0 {
                return Err(HttpResponseParseError::RemoteClosed);
            }
            if !found {
                return if nr < max_len {
                    Err(HttpResponseParseError::RemoteClosed)
                } else {
                    Err(HttpResponseParseError::TooLargeHeader(max_header_size))
                };
            }

            let line_buf = &head_bytes[header_size..];
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }
            rsp.parse_header_line(line_buf)?;
        }

        rsp.origin_header_size = head_bytes.len();
        rsp.post_check_and_fix(method, keep_alive);
        Ok((rsp, head_bytes.freeze()))
    }

    /// do some necessary check and fix
    fn post_check_and_fix(&mut self, method: &Method, request_keep_alive: bool) {
        self.keep_alive = self.connection.keep_alive(self.version) && request_keep_alive;
        if self.original_transfer_encoding_name.is_received() {
            if self.has_content_length || !self.transfer_encoding.chunked() {
                // TE+CL (rfc9112 §6.1) or non-chunked TE (length is the connection)
                self.keep_alive = false;
            }
        } else if !self.has_content_length && !self.expect_no_body(method) {
            // no TE and no CL: read to end and close
            self.keep_alive = false;
        }

        if !self.connection.upgrade() {
            self.upgrade = None;
            self.hop_by_hop_headers.remove(header::UPGRADE);
        }

        // Don't move non-standard connection headers to hop-by-hop headers, as we don't support them
    }

    fn build_from_status_line(line_buf: &[u8]) -> Result<Self, HttpResponseParseError> {
        let rsp =
            HttpStatusLine::parse(line_buf).map_err(HttpResponseParseError::InvalidStatusLine)?;
        let version = match rsp.version {
            0 => Version::HTTP_10,
            1 => Version::HTTP_11,
            2 => return Err(HttpResponseParseError::InvalidVersion(Version::HTTP_2)),
            _ => unreachable!(),
        };

        Ok(HttpTransparentResponse::new(
            version,
            rsp.code,
            rsp.reason.to_owned(),
        ))
    }

    fn parse_header_line(&mut self, line_buf: &[u8]) -> Result<(), HttpResponseParseError> {
        let header =
            HttpHeaderLine::parse(line_buf).map_err(HttpResponseParseError::InvalidHeaderLine)?;
        self.handle_header(header)
    }

    fn insert_hop_by_hop_header(
        &mut self,
        name: HeaderName,
        header: &HttpHeaderLine,
    ) -> Result<(), HttpResponseParseError> {
        let mut value = HttpHeaderValue::from_str(header.value).map_err(|_| {
            HttpResponseParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderValue)
        })?;
        value.set_original_name(header.name);
        self.hop_by_hop_headers.append(name, value);
        Ok(())
    }

    fn handle_header(&mut self, header: HttpHeaderLine) -> Result<(), HttpResponseParseError> {
        let name = HeaderName::from_str(header.name).map_err(|_| {
            HttpResponseParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderName)
        })?;

        match name.as_str() {
            "connection" | "proxy-connection" => {
                // proxy-connection is not standard, but at least curl use it
                self.connection.parse(header.value.as_bytes());
                self.original_connection_name.receive(header.name);
                return Ok(());
            }
            "upgrade" => {
                let protocol = HttpUpgradeToken::from_str(header.value)?;
                self.upgrade = Some(protocol);
                return self.insert_hop_by_hop_header(name, &header);
            }
            "keep-alive" => {
                self.connection
                    .parse_keep_alive(header.name.as_bytes(), header.value.as_bytes());
                return Ok(());
            }
            "transfer-encoding" => {
                self.original_transfer_encoding_name.receive(header.name);
                if self.has_content_length {
                    // Content-Length must be ignored when Transfer-Encoding is present
                    // (RFC 7230 / RFC 9112). Drop the header; keep the flag so
                    // post_check can close per RFC 9112 Section 6.1.
                    self.end_to_end_headers.remove(header::CONTENT_LENGTH);
                    self.content_length = 0;
                }

                self.transfer_encoding
                    .parse(header.value.as_bytes())
                    .map_err(HttpResponseParseError::InvalidTransferEncoding)?;
                return Ok(());
            }
            "content-length" => {
                if self.original_transfer_encoding_name.is_received() {
                    self.has_content_length = true;
                    return Ok(());
                }

                let content_length = u64::from_str(header.value)
                    .map_err(|_| HttpResponseParseError::InvalidContentLength)?;

                if self.has_content_length && self.content_length != content_length {
                    return Err(HttpResponseParseError::InvalidContentLength);
                }
                self.has_content_length = true;
                self.content_length = content_length;
            }
            "proxy-authenticate" => return self.insert_hop_by_hop_header(name, &header),
            _ => {}
        }

        let mut value = HttpHeaderValue::from_str(header.value).map_err(|_| {
            HttpResponseParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderValue)
        })?;
        value.set_original_name(header.name);
        self.end_to_end_headers.append(name, value);
        Ok(())
    }

    pub fn serialize(&self) -> Vec<u8> {
        const RESERVED_LEN_FOR_EXTRA_HEADERS: usize = 256;
        let mut buf =
            Vec::<u8>::with_capacity(self.origin_header_size + RESERVED_LEN_FOR_EXTRA_HEADERS);

        let _ = write!(buf, "{:?} {} {}\r\n", self.version, self.code, self.reason);

        self.end_to_end_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        self.hop_by_hop_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        self.transfer_encoding
            .write(&self.original_transfer_encoding_name, &mut buf);
        self.connection
            .write_for_rsp(&self.original_connection_name, self.keep_alive, &mut buf);
        buf.put_slice(b"\r\n");
        buf
    }

    pub fn serialize_for_adapter(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::with_capacity(self.origin_header_size);

        let _ = write!(buf, "{:?} {} {}\r\n", self.version, self.code, self.reason);

        self.end_to_end_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        buf.put_slice(b"\r\n");
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn read_get() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Date: Fri, 11 Nov 2022 03:22:03 GMT\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            Content-Length: 4\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, data) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert_eq!(data.as_ref(), content.as_slice());
        assert_eq!(rsp.code, 200);
        assert!(rsp.keep_alive());
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::ContentLength(4)));
    }

    #[tokio::test]
    async fn read_get_to_end() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Date: Fri, 11 Nov 2022 03:22:03 GMT\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            Connection: close\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, data) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert_eq!(data.as_ref(), content.as_slice());
        assert_eq!(rsp.code, 200);
        assert!(!rsp.keep_alive());
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::ReadUntilEnd));
    }

    #[tokio::test]
    async fn http10_keep_alive_token_and_request_must_agree() {
        let content = b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\nConnection: Keep-Alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert!(rsp.keep_alive());

        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, false, 4096)
            .await
            .unwrap();
        assert!(!rsp.keep_alive());

        let content = b"HTTP/1.0 200 OK\r\nContent-Length: 0\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert!(!rsp.keep_alive());
    }

    #[tokio::test]
    async fn cl_then_non_chunked_te_reads_until_end() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Content-Length: 6\r\n\
            Transfer-Encoding: gzip\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert!(!rsp.keep_alive());
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::ReadUntilEnd));
        assert!(!rsp.end_to_end_headers.contains_key(header::CONTENT_LENGTH));
    }

    #[tokio::test]
    async fn te_then_cl_ignored_for_non_chunked_te() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: gzip\r\n\
            Content-Length: 6\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert!(!rsp.keep_alive());
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::ReadUntilEnd));
        assert!(!rsp.end_to_end_headers.contains_key(header::CONTENT_LENGTH));
    }

    #[tokio::test]
    async fn cl_then_chunked_te_uses_chunked() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Content-Length: 6\r\n\
            Transfer-Encoding: chunked\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert!(!rsp.keep_alive());
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::Chunked));
        assert!(!rsp.end_to_end_headers.contains_key(header::CONTENT_LENGTH));
    }

    #[tokio::test]
    async fn gzip_then_chunked_is_chunked() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: gzip, chunked\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::Chunked));
    }

    #[tokio::test]
    async fn notchunked_is_rejected() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: notchunked\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        match HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096).await {
            Err(HttpResponseParseError::InvalidTransferEncoding(_)) => {}
            Err(err) => panic!("unexpected error {err:?}"),
            Ok(_) => panic!("expected InvalidTransferEncoding"),
        }
    }

    #[tokio::test]
    async fn chunked_not_last_is_rejected() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: chunked, gzip\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        match HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096).await {
            Err(HttpResponseParseError::InvalidTransferEncoding(_)) => {}
            Err(err) => panic!("unexpected error {err:?}"),
            Ok(_) => panic!("expected InvalidTransferEncoding"),
        }
    }

    #[tokio::test]
    async fn keep_alive_header_is_parsed_and_serialized() {
        let content = b"HTTP/1.0 200 OK\r\n\
            Content-Length: 0\r\n\
            Connection: Keep-Alive\r\n\
            Keep-Alive: timeout=5, max=1000\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        let (rsp, _) = HttpTransparentResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert!(rsp.keep_alive());
        assert_eq!(rsp.keep_alive_header().max(), Some(1000));
        let serialized = String::from_utf8(rsp.serialize()).unwrap();
        assert!(serialized.contains("Keep-Alive: timeout=5, max=1000\r\n"));
    }
}
