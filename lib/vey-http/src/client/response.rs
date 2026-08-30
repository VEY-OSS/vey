/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::str::FromStr;

use bytes::BufMut;
use http::{HeaderName, Method, Version, header};
use tokio::io::AsyncBufRead;

use vey_io_ext::LimitedBufReadExt;
use vey_types::net::{HttpHeaderMap, HttpHeaderValue, TransferEncodingValue};

use super::{HttpAdaptedResponse, HttpResponseParseError};
use crate::header::{Connection, TRANSFER_ENCODING_NAME};
use crate::{HttpBodyType, HttpHeaderLine, HttpLineParseError, HttpStatusLine};

pub struct HttpForwardRemoteResponse {
    pub version: Version,
    pub code: u16,
    pub reason: String,
    pub end_to_end_headers: HttpHeaderMap,
    pub hop_by_hop_headers: HttpHeaderMap,
    original_connection_name: Connection,
    extra_connection_headers: Vec<HeaderName>,
    origin_header_size: usize,
    keep_alive: bool,
    content_length: u64,
    transfer_encoding: TransferEncodingValue,
    original_transfer_encoding_name: Option<[u8; 17]>,
    has_content_length: bool,
    has_keep_alive: bool,
    www_negotiate_auth: bool,
    support_session_based_auth: bool,
}

impl HttpForwardRemoteResponse {
    fn new(version: Version, code: u16, reason: String) -> Self {
        HttpForwardRemoteResponse {
            version,
            code,
            reason,
            end_to_end_headers: HttpHeaderMap::default(),
            hop_by_hop_headers: HttpHeaderMap::default(),
            original_connection_name: Connection::default(),
            extra_connection_headers: Vec::new(),
            origin_header_size: 0,
            keep_alive: false,
            content_length: 0,
            transfer_encoding: TransferEncodingValue::default(),
            original_transfer_encoding_name: None,
            has_content_length: false,
            has_keep_alive: false,
            www_negotiate_auth: false,
            support_session_based_auth: false,
        }
    }

    pub fn adapt_with_body(&self, adapted: HttpAdaptedResponse) -> Self {
        let hop_by_hop_headers = self.hop_by_hop_headers.clone();
        match adapted.content_length {
            Some(content_length) => HttpForwardRemoteResponse {
                version: adapted.version,
                code: adapted.status.as_u16(),
                reason: adapted.reason,
                end_to_end_headers: adapted.headers,
                hop_by_hop_headers,
                original_connection_name: self.original_connection_name.clone(),
                extra_connection_headers: self.extra_connection_headers.clone(),
                origin_header_size: self.origin_header_size,
                keep_alive: self.keep_alive,
                content_length,
                transfer_encoding: TransferEncodingValue::default(),
                original_transfer_encoding_name: None,
                has_content_length: true,
                has_keep_alive: self.has_keep_alive,
                www_negotiate_auth: self.www_negotiate_auth,
                support_session_based_auth: self.support_session_based_auth,
            },
            None => HttpForwardRemoteResponse {
                version: adapted.version,
                code: adapted.status.as_u16(),
                reason: adapted.reason,
                end_to_end_headers: adapted.headers,
                hop_by_hop_headers,
                original_connection_name: self.original_connection_name.clone(),
                extra_connection_headers: self.extra_connection_headers.clone(),
                origin_header_size: self.origin_header_size,
                keep_alive: self.keep_alive,
                content_length: 0,
                transfer_encoding: if self.transfer_encoding.chunked() {
                    self.transfer_encoding
                } else {
                    TransferEncodingValue::CHUNKED
                },
                original_transfer_encoding_name: self
                    .original_transfer_encoding_name
                    .or(Some(TRANSFER_ENCODING_NAME)),
                has_content_length: false,
                has_keep_alive: self.has_keep_alive,
                www_negotiate_auth: self.www_negotiate_auth,
                support_session_based_auth: self.support_session_based_auth,
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
        HttpForwardRemoteResponse {
            version: adapted.version,
            code: adapted.status.as_u16(),
            reason: adapted.reason,
            end_to_end_headers,
            hop_by_hop_headers,
            original_connection_name: self.original_connection_name.clone(),
            extra_connection_headers: self.extra_connection_headers.clone(),
            origin_header_size: self.origin_header_size,
            keep_alive: self.keep_alive,
            content_length: 0,
            transfer_encoding: TransferEncodingValue::default(),
            original_transfer_encoding_name: None,
            has_content_length: true,
            has_keep_alive: self.has_keep_alive,
            www_negotiate_auth: self.www_negotiate_auth,
            support_session_based_auth: self.support_session_based_auth,
        }
    }

    pub fn origin_header_size(&self) -> usize {
        self.origin_header_size
    }

    pub fn keep_alive(&self) -> bool {
        self.keep_alive
    }

    pub fn set_no_keep_alive(&mut self) {
        if self.has_keep_alive {
            self.hop_by_hop_headers
                .remove(HeaderName::from_static("keep-alive"));
            self.has_keep_alive = false;
        }
        self.keep_alive = false;
    }

    #[inline]
    pub fn www_negotiate_auth(&self) -> bool {
        self.www_negotiate_auth
    }

    pub fn set_session_based_auth(&mut self, enable: bool) {
        self.support_session_based_auth = enable;
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
    ) -> Result<Self, HttpResponseParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut line_buf = Vec::<u8>::with_capacity(1024);
        let mut header_size: usize = 0;

        let (found, nr) = reader
            .limited_read_until(b'\n', max_header_size, &mut line_buf)
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
        header_size += nr;

        let mut rsp = HttpForwardRemoteResponse::build_from_status_line(line_buf.as_ref())?;
        rsp.keep_alive = keep_alive;

        loop {
            if header_size >= max_header_size {
                return Err(HttpResponseParseError::TooLargeHeader(max_header_size));
            }
            line_buf.clear();
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_until(b'\n', max_len, &mut line_buf)
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
            header_size += nr;
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }

            rsp.parse_header_line(line_buf.as_ref())?;
        }
        rsp.origin_header_size = header_size;

        rsp.post_check_and_fix(method);
        Ok(rsp)
    }

    /// do some necessary check and fix
    fn post_check_and_fix(&mut self, method: &Method) {
        if !self.transfer_encoding.chunked() {
            if self.expect_no_body(method) {
                // ignore the check of content-length as body is unexpected
            } else if !self.has_content_length {
                // read to end and close the connection
                self.keep_alive = false;
            }
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

        Ok(HttpForwardRemoteResponse::new(
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

    pub fn append_trailer_header(&mut self, name: HeaderName, value: HttpHeaderValue) {
        self.end_to_end_headers.append(name, value);
    }

    fn handle_header(&mut self, header: HttpHeaderLine) -> Result<(), HttpResponseParseError> {
        let name = HeaderName::from_str(header.name).map_err(|_| {
            HttpResponseParseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderName)
        })?;

        match name.as_str() {
            "connection" | "proxy-connection" => {
                // proxy-connection is not standard, but at least curl use it
                let value = header.value.to_lowercase();

                for v in value.as_str().split(',') {
                    if v.is_empty() {
                        continue;
                    }

                    match v.trim() {
                        "keep-alive" => {
                            // keep the original value from request
                        }
                        "close" => {
                            self.keep_alive = false;
                        }
                        s => {
                            if let Ok(h) = HeaderName::from_str(s) {
                                self.extra_connection_headers.push(h);
                            }
                        }
                    }
                }

                self.original_connection_name = Connection::from_original(header.name);
                return Ok(());
            }
            "upgrade" => {
                return self.insert_hop_by_hop_header(name, &header);
            }
            "keep-alive" => {
                // just pass
                self.has_keep_alive = true;
                return self.insert_hop_by_hop_header(name, &header);
            }
            "transfer-encoding" => {
                if self.original_transfer_encoding_name.is_none() {
                    self.original_transfer_encoding_name = Some(
                        header
                            .name
                            .as_bytes()
                            .try_into()
                            .unwrap_or(TRANSFER_ENCODING_NAME),
                    );
                }
                if self.has_content_length {
                    // delete content-length
                    self.end_to_end_headers.remove(header::CONTENT_LENGTH);
                    self.content_length = 0;
                    self.keep_alive = false; // according to rfc9112 Section 6.1
                }

                self.transfer_encoding
                    .parse(header.value.as_bytes())
                    .map_err(HttpResponseParseError::InvalidTransferEncoding)?;
                if !self.transfer_encoding.chunked() {
                    // Non-chunked TE (e.g. "gzip"): message length is delimited by
                    // closing the connection.
                    self.keep_alive = false;
                }
                return Ok(());
            }
            "content-length" => {
                if self.original_transfer_encoding_name.is_some() {
                    // ignore content-length
                    self.keep_alive = false; // according to rfc9112 Section 6.1
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
            "www-authenticate" if header.value.trim_ascii_start().starts_with("Negotiate") => {
                self.www_negotiate_auth = true;
            }
            "proxy-support" => {
                if header.value.to_lowercase() == "session-based-authentication" {
                    self.support_session_based_auth = true;
                }
                return Ok(());
            }
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
        self.serialize_to(&mut buf);
        buf
    }

    pub fn serialize_to(&self, buf: &mut Vec<u8>) {
        let _ = write!(buf, "{:?} {} {}\r\n", self.version, self.code, self.reason);
        self.end_to_end_headers
            .for_each(|name, value| value.write_to_buf(name, buf));
        if self.support_session_based_auth {
            buf.extend_from_slice(b"Proxy-support: Session-Based-Authentication\r\n");
        }
        self.hop_by_hop_headers
            .for_each(|name, value| value.write_to_buf(name, buf));
        self.transfer_encoding.write(
            self.original_transfer_encoding_name
                .as_ref()
                .unwrap_or(&TRANSFER_ENCODING_NAME),
            buf,
        );

        self.original_connection_name.write_to_buf(
            !self.keep_alive,
            &self.extra_connection_headers,
            buf,
        );
        buf.put_slice(b"\r\n");
    }

    pub fn serialize_for_adapter(&self) -> Vec<u8> {
        let mut buf = Vec::<u8>::with_capacity(self.origin_header_size);

        let _ = write!(buf, "{:?} {} {}\r\n", self.version, self.code, self.reason);

        self.end_to_end_headers
            .for_each(|name, value| value.write_to_buf(name, &mut buf));
        if self.support_session_based_auth {
            buf.extend_from_slice(b"Proxy-support: Session-Based-Authentication\r\n");
        }
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
        let rsp = HttpForwardRemoteResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
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
        let rsp = HttpForwardRemoteResponse::parse(&mut buf_stream, &method, true, 4096)
            .await
            .unwrap();
        assert_eq!(rsp.code, 200);
        assert!(!rsp.keep_alive());
        assert_eq!(rsp.body_type(&method), Some(HttpBodyType::ReadUntilEnd));
    }

    #[tokio::test]
    async fn notchunked_is_rejected() {
        let content = b"HTTP/1.1 200 OK\r\n\
            Transfer-Encoding: notchunked\r\n\
            Connection: keep-alive\r\n\r\n";
        let stream = tokio_test::io::Builder::new().read(content).build();
        let mut buf_stream = BufReader::new(stream);
        let method = Method::GET;
        match HttpForwardRemoteResponse::parse(&mut buf_stream, &method, true, 4096).await {
            Err(HttpResponseParseError::InvalidTransferEncoding(_)) => {}
            Err(err) => panic!("unexpected error {err:?}"),
            Ok(_) => panic!("expected InvalidTransferEncoding"),
        }
    }
}
