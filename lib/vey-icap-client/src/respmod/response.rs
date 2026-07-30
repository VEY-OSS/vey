/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::io::AsyncBufRead;

use vey_io_ext::LimitedBufReadExt;

use super::{IcapRespmodParseError, IcapRespmodResponsePayload};
use crate::parse::{HeaderLine, StatusLine};

pub(crate) struct RespmodResponse {
    pub(crate) code: u16,
    pub(crate) reason: String,
    pub(crate) keep_alive: bool,
    pub(crate) payload: IcapRespmodResponsePayload,
}

impl RespmodResponse {
    fn new(code: u16, reason: String) -> Self {
        RespmodResponse {
            code,
            reason,
            keep_alive: true,
            payload: IcapRespmodResponsePayload::NoPayload,
        }
    }

    pub(crate) async fn parse<R>(
        reader: &mut R,
        max_header_size: usize,
    ) -> Result<RespmodResponse, IcapRespmodParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut line_buf = Vec::<u8>::with_capacity(1024);
        let mut header_size = 0;

        let (found, nr) = reader
            .limited_read_until(b'\n', max_header_size, &mut line_buf)
            .await?;
        if nr == 0 {
            return Err(IcapRespmodParseError::RemoteClosed);
        }
        if !found {
            return if nr < max_header_size {
                Err(IcapRespmodParseError::RemoteClosed)
            } else {
                Err(IcapRespmodParseError::TooLargeHeader(max_header_size))
            };
        }
        header_size += nr;
        let mut rsp = Self::build_from_status_line(&line_buf)?;

        loop {
            if header_size >= max_header_size {
                return Err(IcapRespmodParseError::TooLargeHeader(max_header_size));
            }
            line_buf.clear();
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_until(b'\n', max_len, &mut line_buf)
                .await?;
            if nr == 0 {
                return Err(IcapRespmodParseError::RemoteClosed);
            }
            if !found {
                return if nr < max_len {
                    Err(IcapRespmodParseError::RemoteClosed)
                } else {
                    Err(IcapRespmodParseError::TooLargeHeader(max_header_size))
                };
            }
            header_size += nr;
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }

            rsp.parse_header_line(&line_buf)?;
        }

        Ok(rsp)
    }

    fn build_from_status_line(line_buf: &[u8]) -> Result<Self, IcapRespmodParseError> {
        let status =
            StatusLine::parse(line_buf).map_err(IcapRespmodParseError::InvalidStatusLine)?;

        let rsp = RespmodResponse::new(status.code, status.message.to_owned());
        Ok(rsp)
    }

    fn parse_header_line(&mut self, line: &[u8]) -> Result<(), IcapRespmodParseError> {
        let header = HeaderLine::parse(line).map_err(IcapRespmodParseError::InvalidHeaderLine)?;

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
            "encapsulated" => self.payload = IcapRespmodResponsePayload::parse(header.value)?,
            _ => {}
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
        let data = b"ICAP/1.0 204 No Modifications\r\n\
Encapsulated: null-body=0\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        let rsp = RespmodResponse::parse(&mut reader, 8192).await.unwrap();
        assert_eq!(rsp.code, 204);
        assert_eq!(rsp.reason, "No Modifications");
        assert!(rsp.keep_alive);
        assert_eq!(rsp.payload, IcapRespmodResponsePayload::NoPayload);
    }

    #[tokio::test]
    async fn parse_connection_close_and_res_body() {
        let data = b"ICAP/1.0 200 OK\r\n\
Connection: Keep-Alive, close\r\n\
Encapsulated: res-hdr=0, res-body=100\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        let rsp = RespmodResponse::parse(&mut reader, 8192).await.unwrap();
        assert_eq!(rsp.code, 200);
        assert!(!rsp.keep_alive);
        assert_eq!(
            rsp.payload,
            IcapRespmodResponsePayload::HttpResponseWithBody(100)
        );
    }

    #[tokio::test]
    async fn parse_rejects_too_large_header() {
        let data = b"ICAP/1.0 200 OK\r\nEncapsulated: null-body=0\r\n\r\n";
        let mut reader = Cursor::new(&data[..]);
        match RespmodResponse::parse(&mut reader, 8).await {
            Err(IcapRespmodParseError::TooLargeHeader(8)) => {}
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("expected TooLargeHeader"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_remote_closed() {
        let data = b"";
        let mut reader = Cursor::new(&data[..]);
        match RespmodResponse::parse(&mut reader, 8192).await {
            Err(IcapRespmodParseError::RemoteClosed) => {}
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("expected RemoteClosed"),
        }
    }
}
