/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::io::Write;
use std::str::FromStr;

use bytes::{BufMut, Bytes, BytesMut};
use http::{HeaderName, Method, Uri, Version, header};
use tokio::io::AsyncBufRead;

use vey_io_ext::LimitedBufReadExt;
use vey_types::net::http_names;
use vey_types::net::{
    AcceptTransferEncodingValue, ConnectionValue, HttpHeaderMap, HttpHeaderValue,
    HttpKnownHeaderName, HttpUpgradeToken, KeepAliveValue, TransferEncodingValue, UpstreamAddr,
};

use super::{HttpAdaptedRequest, HttpRequestParseError};
use crate::{HttpBodyType, HttpHeaderLine, HttpLineParseError, HttpMethodLine};

pub struct HttpTransparentRequest {
    pub method: Method,
    pub version: Version,
    pub uri: Uri,
    steal_forwarded_for: bool,
    pub end_to_end_headers: HttpHeaderMap,
    pub hop_by_hop_headers: HttpHeaderMap,
    /// the port may be 0
    pub host: Option<UpstreamAddr>,
    original_connection_name: HttpKnownHeaderName<http_names::CONNECTION>,
    connection: ConnectionValue,
    origin_header_size: usize,
    keep_alive: bool,
    pub upgrade: bool,
    content_length: u64,
    transfer_encoding: TransferEncodingValue,
    has_content_length: bool,
    expect_100_continue: bool,
    accept_transfer_encoding: AcceptTransferEncodingValue,
    original_te_name: HttpKnownHeaderName<http_names::TE>,
    original_transfer_encoding_name: HttpKnownHeaderName<http_names::TRANSFER_ENCODING>,
}

impl HttpTransparentRequest {
    fn new(method: Method, uri: Uri, version: Version) -> Self {
        HttpTransparentRequest {
            version,
            method,
            uri,
            steal_forwarded_for: false,
            end_to_end_headers: HttpHeaderMap::default(),
            hop_by_hop_headers: HttpHeaderMap::default(),
            host: None,
            original_connection_name: HttpKnownHeaderName::new(),
            connection: ConnectionValue::default(),
            origin_header_size: 0,
            keep_alive: false,
            upgrade: false,
            content_length: 0,
            transfer_encoding: TransferEncodingValue::default(),
            has_content_length: false,
            expect_100_continue: false,
            accept_transfer_encoding: AcceptTransferEncodingValue::default(),
            original_te_name: HttpKnownHeaderName::new(),
            original_transfer_encoding_name: HttpKnownHeaderName::new(),
        }
    }

    pub fn adapt_with_body(&self, adapted: HttpAdaptedRequest) -> Self {
        let hop_by_hop_headers = self.hop_by_hop_headers.clone();
        match adapted.content_length {
            Some(content_length) => HttpTransparentRequest {
                version: adapted.version,
                method: adapted.method,
                uri: adapted.uri,
                steal_forwarded_for: false,
                end_to_end_headers: adapted.headers,
                hop_by_hop_headers,
                host: None,
                original_connection_name: self.original_connection_name,
                connection: self.connection.clone(),
                origin_header_size: self.origin_header_size,
                keep_alive: self.keep_alive,
                upgrade: self.upgrade,
                content_length,
                transfer_encoding: TransferEncodingValue::default(),
                has_content_length: true,
                expect_100_continue: self.expect_100_continue,
                accept_transfer_encoding: self.accept_transfer_encoding,
                original_te_name: self.original_te_name,
                original_transfer_encoding_name: self.original_transfer_encoding_name.cleared(),
            },
            None => HttpTransparentRequest {
                version: adapted.version,
                method: adapted.method,
                uri: adapted.uri,
                steal_forwarded_for: false,
                end_to_end_headers: adapted.headers,
                hop_by_hop_headers,
                host: None,
                original_connection_name: self.original_connection_name,
                connection: self.connection.clone(),
                origin_header_size: self.origin_header_size,
                keep_alive: self.keep_alive,
                upgrade: self.upgrade,
                content_length: 0,
                transfer_encoding: TransferEncodingValue::CHUNKED,
                has_content_length: false,
                expect_100_continue: self.expect_100_continue,
                accept_transfer_encoding: self.accept_transfer_encoding,
                original_te_name: self.original_te_name,
                original_transfer_encoding_name: self
                    .original_transfer_encoding_name
                    .received_or_default(),
            },
        }
    }

    pub fn adapt_without_body(&self, adapted: HttpAdaptedRequest) -> Self {
        let hop_by_hop_headers = self.hop_by_hop_headers.clone();
        HttpTransparentRequest {
            version: adapted.version,
            method: adapted.method,
            uri: adapted.uri,
            steal_forwarded_for: false,
            end_to_end_headers: adapted.headers,
            hop_by_hop_headers,
            host: None,
            original_connection_name: self.original_connection_name,
            connection: self.connection.clone(),
            origin_header_size: self.origin_header_size,
            keep_alive: self.keep_alive,
            upgrade: self.upgrade,
            content_length: 0,
            transfer_encoding: TransferEncodingValue::default(),
            has_content_length: false,
            expect_100_continue: self.expect_100_continue,
            accept_transfer_encoding: self.accept_transfer_encoding,
            original_te_name: self.original_te_name,
            original_transfer_encoding_name: self.original_transfer_encoding_name.cleared(),
        }
    }

    #[inline]
    pub fn disable_keep_alive(&mut self) {
        self.keep_alive = false;
    }

    #[inline]
    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }

    #[inline]
    pub fn keep_alive_header(&self) -> KeepAliveValue {
        self.connection.keep_alive_header()
    }

    pub fn body_type(&self) -> Option<HttpBodyType> {
        if self.transfer_encoding.chunked() {
            Some(HttpBodyType::Chunked)
        } else if self.content_length > 0 {
            Some(HttpBodyType::ContentLength(self.content_length))
        } else {
            None
        }
    }

    #[inline]
    pub fn expect_100_continue(&self) -> bool {
        self.expect_100_continue
    }

    pub fn pipeline_safe(&self) -> bool {
        if matches!(
            &self.method,
            &Method::GET | &Method::HEAD | &Method::PUT | &Method::DELETE | &Method::QUERY
        ) {
            if self.upgrade {
                return false;
            }
            // only pipeline idempotent requests without body
            if self.body_type().is_none() {
                // reader should not be sent
                return true;
            }
        }
        false
    }

    pub async fn parse<R>(
        reader: &mut R,
        max_header_size: usize,
        steal_forwarded_for: bool,
    ) -> Result<(Self, Bytes), HttpRequestParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut head_bytes = BytesMut::with_capacity(4096);

        let (found, nr) = reader
            .limited_read_buf_until(b'\n', max_header_size, &mut head_bytes)
            .await?;
        if nr == 0 {
            return Err(HttpRequestParseError::ClientClosed);
        }
        if !found {
            return if nr < max_header_size {
                Err(HttpRequestParseError::ClientClosed)
            } else {
                Err(HttpRequestParseError::TooLargeHeader(max_header_size))
            };
        }

        let mut req = HttpTransparentRequest::build_from_method_line(head_bytes.as_ref())?;
        req.steal_forwarded_for = steal_forwarded_for;

        loop {
            let header_size = head_bytes.len();
            if header_size >= max_header_size {
                return Err(HttpRequestParseError::TooLargeHeader(max_header_size));
            }
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_buf_until(b'\n', max_len, &mut head_bytes)
                .await?;
            if nr == 0 {
                return Err(HttpRequestParseError::ClientClosed);
            }
            if !found {
                return if nr < max_len {
                    Err(HttpRequestParseError::ClientClosed)
                } else {
                    Err(HttpRequestParseError::TooLargeHeader(max_header_size))
                };
            }

            let line_buf = &head_bytes[header_size..];
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }
            req.parse_header_line(line_buf)?;
        }

        req.origin_header_size = head_bytes.len();
        req.post_check_and_fix();
        Ok((req, head_bytes.freeze()))
    }

    /// do some necessary check and fix
    fn post_check_and_fix(&mut self) {
        if !self.connection.upgrade() {
            self.upgrade = false;
            self.hop_by_hop_headers.remove(header::UPGRADE);
        }

        self.keep_alive = self.connection.keep_alive(self.version);
        if self.original_transfer_encoding_name.is_received() && self.has_content_length {
            self.keep_alive = false; // according to rfc9112 Section 6.1
        }

        // Don't move non-standard connection headers to hop-by-hop headers, as we don't support them
    }

    fn build_from_method_line(line_buf: &[u8]) -> Result<Self, HttpRequestParseError> {
        let req =
            HttpMethodLine::parse(line_buf).map_err(HttpRequestParseError::InvalidMethodLine)?;
        let version = match req.version {
            0 => Version::HTTP_10,
            1 => Version::HTTP_11,
            2 => return Err(HttpRequestParseError::UnsupportedVersion(Version::HTTP_2)),
            _ => unreachable!(),
        };

        let method = Method::from_str(req.method)
            .map_err(|_| HttpRequestParseError::UnsupportedMethod(req.method.to_owned()))?;
        let uri =
            Uri::from_str(req.uri).map_err(|_| HttpRequestParseError::InvalidRequestTarget)?;
        Ok(HttpTransparentRequest::new(method, uri, version))
    }

    fn parse_header_line(&mut self, line_buf: &[u8]) -> Result<(), HttpRequestParseError> {
        let header =
            HttpHeaderLine::parse(line_buf).map_err(HttpRequestParseError::InvalidHeaderLine)?;
        self.handle_header(header)
    }

    pub fn parse_header_connection(
        &mut self,
        header: &HttpHeaderLine,
    ) -> Result<(), HttpRequestParseError> {
        self.connection.parse(header.value.as_bytes());
        self.original_connection_name.receive(header.name);
        Ok(())
    }

    pub fn append_header(
        &mut self,
        name: HeaderName,
        header: &HttpHeaderLine,
    ) -> Result<(), HttpRequestParseError> {
        let mut value = HttpHeaderValue::from_str(header.value).map_err(|_| {
            HttpRequestParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderValue)
        })?;
        value.set_original_name(header.name);
        self.end_to_end_headers.append(name, value);
        Ok(())
    }

    pub fn retain_upgrade_token<F>(&mut self, retain: F) -> Option<usize>
    where
        F: Fn(&Self, &HttpUpgradeToken) -> bool,
    {
        let mut new_upgrade_headers = Vec::with_capacity(4);
        let mut checked_tokens = BTreeSet::new();
        for header in self.hop_by_hop_headers.get_all(header::UPGRADE) {
            let value = header.to_str();
            for s in value.split(',') {
                let s = s.trim();
                if s.is_empty() {
                    continue;
                }

                let Ok(protocol) = HttpUpgradeToken::from_str(s) else {
                    continue;
                };
                if checked_tokens.contains(&protocol) {
                    continue;
                }
                if retain(self, &protocol) {
                    let mut new_value =
                        unsafe { HttpHeaderValue::from_string_unchecked(s.to_owned()) };
                    if let Some(name) = header.original_name() {
                        new_value.set_original_name(name);
                    }
                    new_upgrade_headers.push(new_value);
                }
                checked_tokens.insert(protocol);
            }
        }

        self.hop_by_hop_headers.remove(header::UPGRADE)?;
        let retain_count = new_upgrade_headers.len();
        for value in new_upgrade_headers {
            self.hop_by_hop_headers.append(header::UPGRADE, value);
        }
        Some(retain_count)
    }

    fn insert_hop_by_hop_header(
        &mut self,
        name: HeaderName,
        header: &HttpHeaderLine,
    ) -> Result<(), HttpRequestParseError> {
        let mut value = HttpHeaderValue::from_str(header.value).map_err(|_| {
            HttpRequestParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderValue)
        })?;
        value.set_original_name(header.name);
        self.hop_by_hop_headers.append(name, value);
        Ok(())
    }

    fn handle_header(&mut self, header: HttpHeaderLine) -> Result<(), HttpRequestParseError> {
        let name = HeaderName::from_str(header.name).map_err(|_| {
            HttpRequestParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderName)
        })?;

        match name.as_str() {
            "host" => {
                if self.host.is_some() {
                    return Err(HttpRequestParseError::InvalidHost);
                }
                if !header.value.is_empty() {
                    let host = UpstreamAddr::from_str(header.value)
                        .map_err(|_| HttpRequestParseError::InvalidHost)?;
                    // we didn't set the default port here, as we didn't know the server port
                    self.host = Some(host);
                }
            }
            "connection" => return self.parse_header_connection(&header),
            "keep-alive" => {
                self.connection
                    .parse_keep_alive(header.name.as_bytes(), header.value.as_bytes());
                return Ok(());
            }
            "upgrade" => {
                self.upgrade = true;
                return self.insert_hop_by_hop_header(name, &header);
            }
            "transfer-encoding" => {
                self.original_transfer_encoding_name.receive(header.name);
                if self.has_content_length {
                    // delete content-length
                    self.end_to_end_headers.remove(header::CONTENT_LENGTH);
                    self.content_length = 0;
                }

                self.transfer_encoding
                    .parse(header.value.as_bytes())
                    .map_err(HttpRequestParseError::InvalidTransferEncoding)?;
                if self.transfer_encoding.body_compressed() {
                    return Err(HttpRequestParseError::UnsupportedTransferEncoding);
                }
                if !self.transfer_encoding.chunked() {
                    return Err(HttpRequestParseError::NotChunkedTransferEncoding);
                }
                return Ok(());
            }
            "content-length" => {
                if self.original_transfer_encoding_name.is_received() {
                    self.has_content_length = true;
                    return Ok(());
                }

                let content_length = u64::from_str(header.value)
                    .map_err(|_| HttpRequestParseError::InvalidContentLength)?;

                if self.has_content_length && self.content_length != content_length {
                    return Err(HttpRequestParseError::InvalidContentLength);
                }
                self.has_content_length = true;
                self.content_length = content_length;
            }
            "te" => {
                self.accept_transfer_encoding
                    .parse(header.value.as_bytes())
                    .map_err(HttpRequestParseError::InvalidAcceptTransferEncoding)?;
                self.original_te_name.receive(header.name);
                return Ok(());
            }
            "proxy-authorization" => {
                return self.insert_hop_by_hop_header(name, &header);
            }
            "forwarded" | "x-forwarded-for" if self.steal_forwarded_for => {
                return Ok(());
            }
            "expect" if header.value == "100-continue" => {
                self.expect_100_continue = true;
            }
            _ => {}
        }

        self.append_header(name, &header)
    }

    pub fn serialize_for_origin(&self) -> Vec<u8> {
        const RESERVED_LEN_FOR_EXTRA_HEADERS: usize = 256;
        let mut buf =
            Vec::<u8>::with_capacity(self.origin_header_size + RESERVED_LEN_FOR_EXTRA_HEADERS);
        if let Some(pa) = self.uri.path_and_query() {
            let _ = write!(buf, "{} {} {:?}\r\n", self.method, pa, self.version);
        } else if self.method.eq(&Method::OPTIONS) {
            let _ = write!(buf, "OPTIONS * {:?}\r\n", self.version);
        } else {
            let _ = write!(buf, "{} / {:?}\r\n", self.method, self.version);
        }
        self.end_to_end_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        self.hop_by_hop_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        self.transfer_encoding
            .write_chunked(&self.original_transfer_encoding_name, &mut buf);
        self.accept_transfer_encoding
            .write_trailers(&self.original_te_name, &mut buf);
        let te = if self.accept_transfer_encoding.trailers() {
            Some(self.original_te_name.as_bytes())
        } else {
            None
        };
        self.connection.write_for_req(
            &self.original_connection_name,
            self.keep_alive,
            te,
            &mut buf,
        );
        buf.put_slice(b"\r\n");
        buf
    }

    pub fn serialize_for_adapter(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::with_capacity(self.origin_header_size);
        if let Some(pa) = self.uri.path_and_query() {
            let _ = write!(buf, "{} {} {:?}\r\n", self.method, pa, self.version);
        } else if self.method.eq(&Method::OPTIONS) {
            let _ = write!(buf, "OPTIONS * {:?}\r\n", self.version);
        } else {
            let _ = write!(buf, "{} / {:?}\r\n", self.method, self.version);
        }
        self.end_to_end_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        buf.put_slice(b"\r\n");
        buf
    }
}

enum HttpTransparentRequestAcceptState {
    RecvMethodLine,
    RecvHeaderLine(HttpTransparentRequest),
    Finished(HttpTransparentRequest),
    End,
}

pub struct HttpTransparentRequestAcceptor {
    state: Option<HttpTransparentRequestAcceptState>,
}

impl Default for HttpTransparentRequestAcceptor {
    fn default() -> Self {
        HttpTransparentRequestAcceptor {
            state: Some(HttpTransparentRequestAcceptState::RecvMethodLine),
        }
    }
}

impl HttpTransparentRequestAcceptor {
    pub fn read_http(&mut self, buf: &[u8]) -> Result<usize, HttpRequestParseError> {
        let mut offset = 0;
        loop {
            match self.state.take() {
                Some(HttpTransparentRequestAcceptState::RecvMethodLine) => {
                    let Some(p) = memchr::memchr(b'\n', buf) else {
                        self.state = Some(HttpTransparentRequestAcceptState::RecvMethodLine);
                        return Ok(offset);
                    };

                    offset += p + 1;

                    let req = HttpTransparentRequest::build_from_method_line(&buf[0..=p])?;
                    self.state = Some(HttpTransparentRequestAcceptState::RecvHeaderLine(req));
                }
                Some(HttpTransparentRequestAcceptState::RecvHeaderLine(mut req)) => {
                    let Some(p) = memchr::memchr(b'\n', &buf[offset..]) else {
                        return Ok(offset);
                    };

                    let start = offset;
                    offset += p + 1;

                    let line_buf = &buf[start..offset];
                    if (line_buf.len() == 1 && line_buf[0] == b'\n')
                        || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
                    {
                        self.state = Some(HttpTransparentRequestAcceptState::Finished(req))
                    } else {
                        req.parse_header_line(line_buf)?;
                        self.state = Some(HttpTransparentRequestAcceptState::RecvHeaderLine(req))
                    }
                }
                Some(state) => {
                    self.state = Some(state);
                    return Ok(offset);
                }
                None => unreachable!(),
            }
        }
    }

    pub fn accept(&mut self) -> Option<HttpTransparentRequest> {
        let state = self.state.take();
        if let Some(HttpTransparentRequestAcceptState::Finished(req)) = state {
            self.state = Some(HttpTransparentRequestAcceptState::End);
            Some(req)
        } else {
            self.state = state;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn read_get() {
        let content = b"GET http://example.com/v/a/x HTTP/1.1\r\n\
            Host: example.com\r\n\
            Connection: Keep-Alive\r\n\
            Accept-Language: en-us,en;q=0.5\r\n\
            Accept-Encoding: gzip, deflate\r\n\
            Accept: */*\r\n\
            User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like G\
            ecko) Chrome/72.0.3611.2 Safari/537.36\r\n\
            Accept-Charset: ISO-8859-1,utf-8;q=0.7,*;q=0.7\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, data) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert_eq!(data.as_ref(), content.as_slice());
        assert_eq!(request.method, &Method::GET);
        assert!(request.keep_alive());
        assert!(request.body_type().is_none());

        let result = HttpTransparentRequest::parse(&mut buf_stream, 4096, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn connection_close() {
        let content = b"GET http://api.example.com/v1/files?api_key=abcd&ids=xyz HTTP/1.1\r\n\
            Accept: application/json, text/plain, */*\r\n\
            User-Agent: axios/0.21.1\r\n\
            host: api.giphy.com\r\n\
            Connection: close\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, data) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert_eq!(data.as_ref(), content.as_slice());
        assert!(!request.keep_alive());
    }

    #[tokio::test]
    async fn connection_upgrade() {
        let content = b"GET /hello.txt HTTP/1.1\r\n\
            Host: www.example.com\r\n\
            Connection: upgrade\r\n\
            Upgrade: Websocket,  HTTP/2.0\r\n\
            \r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (mut request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        let left_tokens = request
            .retain_upgrade_token(|_req, p| matches!(p, HttpUpgradeToken::Http(_)))
            .unwrap();
        assert_eq!(left_tokens, 1);
        let token = request.hop_by_hop_headers.get(header::UPGRADE).unwrap();
        assert_eq!(token.to_str(), "HTTP/2.0");
    }

    #[tokio::test]
    async fn cl_before_te_drops_content_length() {
        let content = b"POST /x HTTP/1.1\r\n\
            Host: example.com\r\n\
            Content-Length: 6\r\n\
            Transfer-Encoding: chunked\r\n\
            Connection: Keep-Alive\r\n\
            \r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert!(!request.keep_alive());
        assert!(request.body_type().is_some());
        let origin = request.serialize_for_origin();
        let origin = std::str::from_utf8(&origin).unwrap();
        assert!(
            request
                .hop_by_hop_headers
                .get(header::TRANSFER_ENCODING)
                .is_none()
        );
        assert!(origin.contains("Transfer-Encoding: chunked\r\n"));
        assert!(!origin.to_ascii_lowercase().contains("content-length"));
    }

    #[tokio::test]
    async fn te_before_cl_ignores_content_length() {
        let content = b"POST /x HTTP/1.1\r\n\
            Host: example.com\r\n\
            Transfer-Encoding: chunked\r\n\
            Content-Length: 6\r\n\
            Connection: Keep-Alive\r\n\
            \r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert!(!request.keep_alive());
        let origin = request.serialize_for_origin();
        let origin = std::str::from_utf8(&origin).unwrap();
        assert!(origin.contains("Transfer-Encoding: chunked\r\n"));
        assert!(!origin.to_ascii_lowercase().contains("content-length"));
    }

    #[tokio::test]
    async fn transfer_encoding_rejects_suffix_lookalikes() {
        for te in ["notchunked", "foochunked", "chunked, gzip"] {
            let content =
                format!("POST /x HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: {te}\r\n\r\n");
            let stream = tokio_test::io::Builder::new()
                .read(content.as_bytes())
                .build();
            let mut buf_stream = BufReader::new(stream);
            let err = match HttpTransparentRequest::parse(&mut buf_stream, 4096, false).await {
                Err(e) => e,
                Ok(_) => panic!("{te}: expected invalid transfer-encoding"),
            };
            assert!(
                matches!(err, HttpRequestParseError::InvalidTransferEncoding(_)),
                "{te}: {err:?}"
            );
        }

        let content =
            b"POST /x HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: identity\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        match HttpTransparentRequest::parse(&mut buf_stream, 4096, false).await {
            Err(HttpRequestParseError::NotChunkedTransferEncoding) => {}
            Err(err) => panic!("identity: unexpected error {err:?}"),
            Ok(_) => panic!("identity: expected NotChunkedTransferEncoding"),
        }

        for te in ["gzip", "gzip, chunked", "deflate, chunked"] {
            let content =
                format!("POST /x HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: {te}\r\n\r\n");
            let stream = tokio_test::io::Builder::new()
                .read(content.as_bytes())
                .build();
            let mut buf_stream = BufReader::new(stream);
            match HttpTransparentRequest::parse(&mut buf_stream, 4096, false).await {
                Err(HttpRequestParseError::UnsupportedTransferEncoding) => {}
                Err(err) => panic!("{te}: unexpected error {err:?}"),
                Ok(_) => panic!("{te}: expected UnsupportedTransferEncoding"),
            }
        }

        let content =
            b"POST /x HTTP/1.1\r\nHost: example.com\r\nTransfer-Encoding: identity, chunked\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert_eq!(request.body_type(), Some(HttpBodyType::Chunked));
        assert!(
            request
                .hop_by_hop_headers
                .get(header::TRANSFER_ENCODING)
                .is_none()
        );
        let origin = String::from_utf8(request.serialize_for_origin()).unwrap();
        assert!(origin.contains("Transfer-Encoding: chunked\r\n"));
    }

    #[tokio::test]
    async fn te_header_keeps_only_trailers() {
        let content = b"GET /x HTTP/1.1\r\nHost: example.com\r\nTE: gzip, trailers;q=1.0\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert!(request.hop_by_hop_headers.get(header::TE).is_none());
        let origin = String::from_utf8(request.serialize_for_origin()).unwrap();
        assert!(origin.contains("TE: trailers\r\n"));

        let content = b"GET /x HTTP/1.1\r\nHost: example.com\r\nTE: deflate\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert!(request.hop_by_hop_headers.get(header::TE).is_none());
        let origin = String::from_utf8(request.serialize_for_origin())
            .unwrap()
            .to_ascii_lowercase();
        assert!(!origin.contains("\nte:"));
    }

    #[tokio::test]
    async fn keep_alive_header_is_parsed_and_serialized() {
        let content = b"GET /x HTTP/1.0\r\n\
            Host: example.com\r\n\
            Connection: keep-alive\r\n\
            Keep-Alive: timeout=5, max=1000\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let (request, _) = HttpTransparentRequest::parse(&mut buf_stream, 4096, false)
            .await
            .unwrap();
        assert!(request.keep_alive());
        assert_eq!(
            request.keep_alive_header().timeout(),
            Some(std::time::Duration::from_secs(5))
        );
        assert_eq!(request.keep_alive_header().max(), Some(1000));
        let origin = String::from_utf8(request.serialize_for_origin()).unwrap();
        assert!(origin.contains("Keep-Alive: timeout=5, max=1000\r\n"));
    }
}
