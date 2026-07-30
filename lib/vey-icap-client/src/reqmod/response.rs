/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::str::FromStr;

use http::HeaderName;
use tokio::io::AsyncBufRead;

use vey_io_ext::LimitedBufReadExt;
use vey_types::net::{HttpHeaderMap, HttpHeaderValue};

use super::{IcapReqmodParseError, IcapReqmodResponsePayload};
use crate::parse::{HeaderLine, IcapLineParseError, StatusLine};

pub(crate) struct ReqmodResponse {
    pub(crate) code: u16,
    pub(crate) reason: String,
    pub(crate) keep_alive: bool,
    pub(crate) payload: IcapReqmodResponsePayload,
    shared_headers: HttpHeaderMap,
}

impl ReqmodResponse {
    fn new(code: u16, reason: String) -> Self {
        ReqmodResponse {
            code,
            reason,
            keep_alive: true,
            payload: IcapReqmodResponsePayload::NoPayload,
            shared_headers: HttpHeaderMap::default(),
        }
    }

    pub(crate) fn take_shared_headers(&mut self) -> HttpHeaderMap {
        std::mem::take(&mut self.shared_headers)
    }

    pub(crate) async fn parse<R>(
        reader: &mut R,
        max_header_size: usize,
        shared_names: &BTreeSet<String>,
    ) -> Result<ReqmodResponse, IcapReqmodParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut line_buf = Vec::<u8>::with_capacity(1024);
        let mut header_size = 0;

        let (found, nr) = reader
            .limited_read_until(b'\n', max_header_size, &mut line_buf)
            .await?;
        if nr == 0 {
            return Err(IcapReqmodParseError::RemoteClosed);
        }
        if !found {
            return if nr < max_header_size {
                Err(IcapReqmodParseError::RemoteClosed)
            } else {
                Err(IcapReqmodParseError::TooLargeHeader(max_header_size))
            };
        }
        header_size += nr;
        let mut rsp = Self::build_from_status_line(&line_buf)?;

        loop {
            if header_size >= max_header_size {
                return Err(IcapReqmodParseError::TooLargeHeader(max_header_size));
            }
            line_buf.clear();
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_until(b'\n', max_len, &mut line_buf)
                .await?;
            if nr == 0 {
                return Err(IcapReqmodParseError::RemoteClosed);
            }
            if !found {
                return if nr < max_len {
                    Err(IcapReqmodParseError::RemoteClosed)
                } else {
                    Err(IcapReqmodParseError::TooLargeHeader(max_header_size))
                };
            }
            header_size += nr;
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }

            rsp.parse_header_line(&line_buf, shared_names)?;
        }

        Ok(rsp)
    }

    fn build_from_status_line(line_buf: &[u8]) -> Result<Self, IcapReqmodParseError> {
        let status =
            StatusLine::parse(line_buf).map_err(IcapReqmodParseError::InvalidStatusLine)?;

        let rsp = ReqmodResponse::new(status.code, status.message.to_owned());
        Ok(rsp)
    }

    fn parse_header_line(
        &mut self,
        line: &[u8],
        shared_names: &BTreeSet<String>,
    ) -> Result<(), IcapReqmodParseError> {
        let header = HeaderLine::parse(line).map_err(IcapReqmodParseError::InvalidHeaderLine)?;

        match header.name.to_lowercase().as_str() {
            "connection" => {
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
                        _ => {} // ignore other custom hop-by-hop headers
                    }
                }
            }
            "encapsulated" => self.payload = IcapReqmodResponsePayload::parse(header.value)?,
            header_name => {
                if shared_names.contains(header_name) {
                    let name = HeaderName::from_str(header_name).map_err(|_| {
                        IcapReqmodParseError::InvalidHeaderLine(
                            IcapLineParseError::InvalidHeaderName,
                        )
                    })?;
                    let value = HttpHeaderValue::from_str(header.value).map_err(|_| {
                        IcapReqmodParseError::InvalidHeaderLine(
                            IcapLineParseError::InvalidHeaderValue,
                        )
                    })?;
                    self.shared_headers.append(name, value);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn parse_null_body_response() {
        let data = b"ICAP/1.0 204 No Content\r\n\
Encapsulated: null-body=0\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        let rsp = ReqmodResponse::parse(&mut reader, 8192, &BTreeSet::new())
            .await
            .unwrap();
        assert_eq!(rsp.code, 204);
        assert_eq!(rsp.reason, "No Content");
        assert!(rsp.keep_alive);
        assert_eq!(rsp.payload, IcapReqmodResponsePayload::NoPayload);
    }

    #[tokio::test]
    async fn parse_connection_close_and_req_body() {
        let data = b"ICAP/1.0 200 OK\r\n\
Connection: close\r\n\
Encapsulated: req-hdr=0, req-body=42\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        let rsp = ReqmodResponse::parse(&mut reader, 8192, &BTreeSet::new())
            .await
            .unwrap();
        assert_eq!(rsp.code, 200);
        assert!(!rsp.keep_alive);
        assert_eq!(
            rsp.payload,
            IcapReqmodResponsePayload::HttpRequestWithBody(42)
        );
    }

    #[tokio::test]
    async fn parse_collects_shared_headers() {
        let mut shared = BTreeSet::new();
        shared.insert("x-virus-id".to_string());
        let data = b"ICAP/1.0 200 OK\r\n\
X-Virus-ID: clamav\r\n\
X-Ignored: skip\r\n\
Encapsulated: null-body=0\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        let mut rsp = ReqmodResponse::parse(&mut reader, 8192, &shared)
            .await
            .unwrap();
        let headers = rsp.take_shared_headers();
        assert!(headers.contains_key(&HeaderName::from_static("x-virus-id")));
        assert!(!headers.contains_key(&HeaderName::from_static("x-ignored")));
    }

    #[tokio::test]
    async fn parse_rejects_too_large_header() {
        let data = b"ICAP/1.0 200 OK\r\nEncapsulated: null-body=0\r\n\r\n";
        let mut reader = Cursor::new(&data[..]);
        match ReqmodResponse::parse(&mut reader, 8, &BTreeSet::new()).await {
            Err(IcapReqmodParseError::TooLargeHeader(8)) => {}
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("expected TooLargeHeader"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_remote_closed() {
        let data = b"";
        let mut reader = Cursor::new(&data[..]);
        match ReqmodResponse::parse(&mut reader, 8192, &BTreeSet::new()).await {
            Err(IcapReqmodParseError::RemoteClosed) => {}
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("expected RemoteClosed"),
        }
    }
}
