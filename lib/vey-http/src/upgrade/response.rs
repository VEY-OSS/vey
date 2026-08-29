/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use http::HeaderName;
use tokio::io::AsyncBufRead;

use vey_io_ext::LimitedBufReadExt;
use vey_types::net::{HttpHeaderMap, HttpHeaderValue};

use super::{HttpUpgradeError, HttpUpgradeResponseError};
use crate::{HttpBodyReader, HttpBodyType, HttpHeaderLine, HttpLineParseError, HttpStatusLine};

#[derive(Debug)]
pub struct HttpUpgradeResponse {
    pub code: u16,
    pub reason: String,
    pub headers: HttpHeaderMap,
    protocol: &'static str,
    content_length: u64,
    chunked_transfer: bool,
    has_transfer_encoding: bool,
    has_content_length: bool,
}

impl HttpUpgradeResponse {
    fn new(code: u16, reason: String, protocol: &'static str) -> Self {
        HttpUpgradeResponse {
            code,
            reason,
            headers: HttpHeaderMap::default(),
            protocol,
            content_length: 0,
            chunked_transfer: false,
            has_transfer_encoding: false,
            has_content_length: false,
        }
    }

    fn body_type(&self) -> Option<HttpBodyType> {
        if self.chunked_transfer {
            Some(HttpBodyType::Chunked)
        } else if self.content_length > 0 {
            Some(HttpBodyType::ContentLength(self.content_length))
        } else {
            None
        }
    }

    async fn parse<R, F>(
        reader: &mut R,
        protocol: &'static str,
        max_header_size: usize,
        parse_more_header: &mut F,
    ) -> Result<Self, HttpUpgradeError>
    where
        R: AsyncBufRead + Unpin,
        F: Fn(&mut Self, HeaderName, &HttpHeaderLine) -> Result<(), HttpUpgradeResponseError>,
    {
        let mut line_buf = Vec::<u8>::with_capacity(1024);
        let mut header_size: usize = 0;

        let (found, nr) = reader
            .limited_read_until(b'\n', max_header_size, &mut line_buf)
            .await
            .map_err(HttpUpgradeError::ReadFailed)?;
        if nr == 0 {
            return Err(HttpUpgradeError::RemoteClosed);
        }
        if !found {
            return if nr < max_header_size {
                Err(HttpUpgradeError::RemoteClosed)
            } else {
                Err(HttpUpgradeResponseError::TooLargeHeader(max_header_size).into())
            };
        }
        header_size += nr;

        let mut rsp = HttpUpgradeResponse::build_from_status_line(line_buf.as_ref(), protocol)?;

        loop {
            if header_size >= max_header_size {
                return Err(HttpUpgradeResponseError::TooLargeHeader(max_header_size).into());
            }
            line_buf.clear();
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_until(b'\n', max_len, &mut line_buf)
                .await
                .map_err(HttpUpgradeError::ReadFailed)?;
            if nr == 0 {
                return Err(HttpUpgradeError::RemoteClosed);
            }
            if !found {
                return if nr < max_len {
                    Err(HttpUpgradeError::RemoteClosed)
                } else {
                    Err(HttpUpgradeResponseError::TooLargeHeader(max_header_size).into())
                };
            }
            header_size += nr;
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }

            rsp.parse_header_line(line_buf.as_ref(), parse_more_header)?;
        }

        rsp.post_check_and_fix();
        Ok(rsp)
    }

    /// do some necessary check and fix
    fn post_check_and_fix(&mut self) {
        // Don't move non-standard connection headers to hop-by-hop headers, as we don't support them
    }

    fn build_from_status_line(
        line_buf: &[u8],
        protocol: &'static str,
    ) -> Result<Self, HttpUpgradeResponseError> {
        let rsp =
            HttpStatusLine::parse(line_buf).map_err(HttpUpgradeResponseError::InvalidStatusLine)?;
        Ok(HttpUpgradeResponse::new(
            rsp.code,
            rsp.reason.to_owned(),
            protocol,
        ))
    }

    fn parse_header_line<F>(
        &mut self,
        line_buf: &[u8],
        parse_more_header: &mut F,
    ) -> Result<(), HttpUpgradeResponseError>
    where
        F: Fn(&mut Self, HeaderName, &HttpHeaderLine) -> Result<(), HttpUpgradeResponseError>,
    {
        let header =
            HttpHeaderLine::parse(line_buf).map_err(HttpUpgradeResponseError::InvalidHeaderLine)?;
        self.handle_header(header, parse_more_header)
    }

    fn handle_header<F>(
        &mut self,
        header: HttpHeaderLine,
        parse_more_header: &mut F,
    ) -> Result<(), HttpUpgradeResponseError>
    where
        F: Fn(&mut Self, HeaderName, &HttpHeaderLine) -> Result<(), HttpUpgradeResponseError>,
    {
        let name = HeaderName::from_str(header.name).map_err(|_| {
            HttpUpgradeResponseError::InvalidHeaderLine(HttpLineParseError::InvalidHeaderName)
        })?;

        match name.as_str() {
            "connection" => {
                if header.value.to_lowercase() != "upgrade" {
                    return Err(HttpUpgradeResponseError::UnsupportedHeaderValue(
                        "Connection",
                    ));
                }
                return Ok(());
            }
            "upgrade" => {
                if header.value != self.protocol {
                    return Err(HttpUpgradeResponseError::UpgradeTokenNotMatch);
                }
                return Ok(());
            }
            "transfer-encoding" => {
                self.has_transfer_encoding = true;
                if self.has_content_length {
                    // delete content-length
                    self.content_length = 0;
                }

                let v = header.value.to_lowercase();
                if v.ends_with("chunked") {
                    self.chunked_transfer = true;
                } else if v.contains("chunked") {
                    return Err(HttpUpgradeResponseError::InvalidChunkedTransferEncoding);
                }
            }
            "content-length" => {
                if self.has_transfer_encoding {
                    // ignore content-length
                    return Ok(());
                }

                let content_length = u64::from_str(header.value)
                    .map_err(|_| HttpUpgradeResponseError::InvalidContentLength)?;

                if self.has_content_length && self.content_length != content_length {
                    return Err(HttpUpgradeResponseError::InvalidContentLength);
                }
                self.has_content_length = true;
                self.content_length = content_length;
            }
            _ => {}
        }

        parse_more_header(self, name, &header)
    }

    fn detect_error(&self) -> Result<(), HttpUpgradeError> {
        if self.code == 101 {
            Ok(())
        } else if self.code == 504 || self.code == 522 || self.code == 524 {
            // Peer tells us it times out
            Err(HttpUpgradeError::PeerTimeout(self.code))
        } else {
            Err(HttpUpgradeError::UnexpectedStatusCode(
                self.code,
                self.reason.to_owned(),
            ))
        }
    }

    async fn recv<R, F>(
        r: &mut R,
        protocol: &'static str,
        max_header_size: usize,
        parse_more_header: &mut F,
    ) -> Result<Self, HttpUpgradeError>
    where
        R: AsyncBufRead + Unpin,
        F: Fn(&mut Self, HeaderName, &HttpHeaderLine) -> Result<(), HttpUpgradeResponseError>,
    {
        let rsp =
            HttpUpgradeResponse::parse(r, protocol, max_header_size, parse_more_header).await?;

        if let Some(body_type) = rsp.body_type() {
            // the body should be simple in non-101 case, use a default 2048 for its max line size
            let mut body_reader = HttpBodyReader::new(r, body_type, 2048);
            let mut sink = tokio::io::sink();
            tokio::io::copy(&mut body_reader, &mut sink)
                .await
                .map_err(HttpUpgradeError::ReadFailed)?;
        }

        rsp.detect_error()?;

        Ok(rsp)
    }

    pub async fn recv_for_connect_udp<R>(
        r: &mut R,
        max_header_size: usize,
    ) -> Result<Self, HttpUpgradeError>
    where
        R: AsyncBufRead + Unpin,
    {
        Self::recv(
            r,
            "connect-udp",
            max_header_size,
            &mut |rsp, name, header| {
                if name.as_str() == "capsule-protocol" && !header.value.starts_with("?1") {
                    return Err(HttpUpgradeResponseError::UnsupportedHeaderValue(
                        "Capsule-Protocol",
                    ));
                }
                let value = HttpHeaderValue::from_str(header.value).map_err(|_| {
                    HttpUpgradeResponseError::InvalidHeaderLine(
                        HttpLineParseError::InvalidHeaderValue,
                    )
                })?;
                rsp.headers.append(name, value);
                Ok(())
            },
        )
        .await
    }
}
