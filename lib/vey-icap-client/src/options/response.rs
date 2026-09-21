/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::time::Duration;

use tokio::io::AsyncBufRead;
use tokio::time::Instant;

use vey_http::HttpBodyReader;
use vey_io_ext::LimitedBufReadExt;

use super::IcapOptionsParseError;
use crate::IcapMethod;
use crate::parse::{HeaderLine, StatusLine};

pub struct IcapServiceOptions {
    method: IcapMethod,
    server: Option<String>,
    service_tag: String,
    service_id: Option<String>,
    max_connections: Option<usize>,
    expire: Option<Instant>,
    pub(crate) support_204: bool,
    pub(crate) support_206: bool,
    pub(crate) preview_size: Option<usize>,
    has_opt_body: bool,
}

impl IcapServiceOptions {
    pub(crate) fn new(method: IcapMethod) -> Self {
        IcapServiceOptions {
            method,
            server: None,
            service_tag: String::new(),
            service_id: None,
            max_connections: None,
            expire: None,
            support_204: false,
            support_206: false,
            preview_size: None,
            has_opt_body: false,
        }
    }

    pub(crate) fn new_expired(method: IcapMethod) -> Self {
        IcapServiceOptions {
            method,
            server: None,
            service_tag: String::new(),
            service_id: None,
            max_connections: None,
            expire: Some(Instant::now()),
            support_204: false,
            support_206: false,
            preview_size: None,
            has_opt_body: false,
        }
    }

    pub(crate) fn expired(&self) -> bool {
        if let Some(expire) = self.expire {
            Instant::now() >= expire
        } else {
            false
        }
    }

    pub(crate) async fn parse<R>(
        reader: &mut R,
        method: IcapMethod,
        max_header_size: usize,
    ) -> Result<IcapServiceOptions, IcapOptionsParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut options = IcapServiceOptions::new(method);

        let mut line_buf = Vec::<u8>::with_capacity(1024);
        let mut header_size = 0;

        let (found, nr) = reader
            .limited_read_until(b'\n', max_header_size, &mut line_buf)
            .await?;
        if nr == 0 {
            return Err(IcapOptionsParseError::RemoteClosed);
        }
        if !found {
            return if nr < max_header_size {
                Err(IcapOptionsParseError::RemoteClosed)
            } else {
                Err(IcapOptionsParseError::TooLargeHeader(max_header_size))
            };
        }
        header_size += nr;
        options.parse_status_line(&line_buf)?;

        loop {
            if header_size >= max_header_size {
                return Err(IcapOptionsParseError::TooLargeHeader(max_header_size));
            }
            line_buf.clear();
            let max_len = max_header_size - header_size;
            let (found, nr) = reader
                .limited_read_until(b'\n', max_len, &mut line_buf)
                .await?;
            if nr == 0 {
                return Err(IcapOptionsParseError::RemoteClosed);
            }
            if !found {
                return if nr < max_len {
                    Err(IcapOptionsParseError::RemoteClosed)
                } else {
                    Err(IcapOptionsParseError::TooLargeHeader(max_header_size))
                };
            }
            header_size += nr;
            if (line_buf.len() == 1 && line_buf[0] == b'\n')
                || (line_buf.len() == 2 && line_buf[0] == b'\r' && line_buf[1] == b'\n')
            {
                // header end line
                break;
            }

            options.parse_header_line(&line_buf)?;
        }
        options.check()?;

        if options.has_opt_body {
            Self::consume_opt_body(reader).await?;
        }

        Ok(options)
    }

    async fn consume_opt_body<R>(reader: &mut R) -> Result<(), IcapOptionsParseError>
    where
        R: AsyncBufRead + Unpin,
    {
        const BODY_LINE_MAX_LEN: usize = 1024;
        let mut body_reader = HttpBodyReader::new_chunked(reader, BODY_LINE_MAX_LEN);
        tokio::io::copy(&mut body_reader, &mut tokio::io::sink()).await?;
        if !body_reader.finished() {
            return Err(IcapOptionsParseError::RemoteClosed);
        }
        Ok(())
    }

    fn check(&self) -> Result<(), IcapOptionsParseError> {
        if self.service_tag.is_empty() {
            return Err(IcapOptionsParseError::NoServiceTagSet);
        }
        Ok(())
    }

    fn parse_status_line(&mut self, line: &[u8]) -> Result<(), IcapOptionsParseError> {
        let status = StatusLine::parse(line).map_err(IcapOptionsParseError::InvalidStatusLine)?;

        if status.code < 200 || status.code >= 300 {
            return Err(IcapOptionsParseError::RequestFailed(
                status.code,
                status.message.to_owned(),
            ));
        }

        Ok(())
    }

    fn parse_header_line(&mut self, line: &[u8]) -> Result<(), IcapOptionsParseError> {
        let header = HeaderLine::parse(line).map_err(IcapOptionsParseError::InvalidHeaderLine)?;

        match header.name.to_lowercase().as_str() {
            "methods" => {
                for v in header.value.split(',') {
                    if self.method.as_str() == v.trim() {
                        return Ok(());
                    }
                }
                return Err(IcapOptionsParseError::MethodNotMatch);
            }
            "service" => self.server = Some(header.value.to_owned()),
            "istag" => self.service_tag = header.value.to_owned(),
            "encapsulated" => {
                let mut saw_null_body = false;
                let mut saw_opt_body = false;
                for p in header.value.split(',') {
                    let Some((name, value)) = p.trim().split_once('=') else {
                        return Err(IcapOptionsParseError::InvalidHeaderValue("Encapsulated"));
                    };
                    match name.to_lowercase().as_str() {
                        "null-body" => {
                            if saw_null_body || saw_opt_body {
                                return Err(IcapOptionsParseError::InvalidHeaderValue(
                                    "Encapsulated",
                                ));
                            }
                            saw_null_body = true;
                        }
                        "opt-body" => {
                            if saw_null_body || saw_opt_body || value != "0" {
                                return Err(IcapOptionsParseError::InvalidHeaderValue(
                                    "Encapsulated",
                                ));
                            }
                            saw_opt_body = true;
                        }
                        _ => {
                            return Err(IcapOptionsParseError::InvalidHeaderValue("Encapsulated"));
                        }
                    }
                }
                if !saw_null_body && !saw_opt_body {
                    return Err(IcapOptionsParseError::InvalidHeaderValue("Encapsulated"));
                }
                self.has_opt_body = saw_opt_body;
            }
            "opt-body-type" => {}
            "max-connections" => {
                let max_connections = usize::from_str(header.value)
                    .map_err(|_| IcapOptionsParseError::InvalidHeaderValue("Max-Connections"))?;
                self.max_connections = Some(max_connections);
            }
            "options-ttl" => {
                let ttl = usize::from_str(header.value)
                    .map_err(|_| IcapOptionsParseError::InvalidHeaderValue("Options-TTL"))?;
                let expire = Instant::now()
                    .checked_add(Duration::from_secs(ttl as u64))
                    .ok_or(IcapOptionsParseError::InvalidHeaderValue("Options-TTL"))?;
                self.expire = Some(expire);
            }
            "service-id" => self.service_id = Some(header.value.to_owned()),
            "allow" => {
                for p in header.value.split(',') {
                    let code = u16::from_str(p.trim())
                        .map_err(|_| IcapOptionsParseError::InvalidHeaderValue("Allow"))?;
                    match code {
                        204 => self.support_204 = true,
                        206 => self.support_206 = true,
                        _ => {}
                    }
                }
            }
            "preview" => {
                let size = usize::from_str(header.value)
                    .map_err(|_| IcapOptionsParseError::InvalidHeaderValue("Preview"))?;
                self.preview_size = Some(size);
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_ttl_rejects_overflowing_value() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        let err = options
            .parse_header_line(b"Options-TTL: 18446744073709551615\r\n")
            .unwrap_err();
        assert!(matches!(
            err,
            IcapOptionsParseError::InvalidHeaderValue("Options-TTL")
        ));
    }

    #[test]
    fn options_ttl_accepts_reasonable_value() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options.parse_header_line(b"Options-TTL: 3600\r\n").unwrap();
        assert!(options.expire.is_some());
        assert!(!options.expired());
    }

    #[test]
    fn parse_header_allow_sets_support_flags() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options.parse_header_line(b"Allow: 204, 206\r\n").unwrap();
        assert!(options.support_204);
        assert!(options.support_206);
    }

    #[test]
    fn parse_header_preview_sets_size() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options.parse_header_line(b"Preview: 4096\r\n").unwrap();
        assert_eq!(options.preview_size, Some(4096));
    }

    #[test]
    fn parse_header_methods_must_match() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_header_line(b"Methods: RESPMOD\r\n"),
            Err(IcapOptionsParseError::MethodNotMatch)
        ));
    }

    #[test]
    fn parse_header_encapsulated_accepts_null_body() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options
            .parse_header_line(b"Encapsulated: null-body=0\r\n")
            .unwrap();
        assert!(!options.has_opt_body);
    }

    #[test]
    fn parse_header_encapsulated_accepts_opt_body() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options
            .parse_header_line(b"Encapsulated: opt-body=0\r\n")
            .unwrap();
        assert!(options.has_opt_body);
    }

    #[test]
    fn parse_header_encapsulated_rejects_opt_body_nonzero_offset() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_header_line(b"Encapsulated: opt-body=42\r\n"),
            Err(IcapOptionsParseError::InvalidHeaderValue("Encapsulated"))
        ));
    }

    #[test]
    fn parse_header_encapsulated_rejects_null_and_opt_body() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_header_line(b"Encapsulated: null-body=0, opt-body=0\r\n"),
            Err(IcapOptionsParseError::InvalidHeaderValue("Encapsulated"))
        ));
    }

    #[test]
    fn check_requires_istag() {
        let options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.check(),
            Err(IcapOptionsParseError::NoServiceTagSet)
        ));
    }

    #[test]
    fn parse_header_methods_accepts_matching_method() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options
            .parse_header_line(b"Methods: OPTIONS, REQMOD, RESPMOD\r\n")
            .unwrap();
    }

    #[test]
    fn parse_header_service_and_istag() {
        let mut options = IcapServiceOptions::new(IcapMethod::Respmod);
        options
            .parse_header_line(b"Service: Example ICAP Server 1.0\r\n")
            .unwrap();
        options
            .parse_header_line(b"ISTag: \"W3E4R7U9-L3E4R7U9-W3E4R7U9\"\r\n")
            .unwrap();
        options
            .parse_header_line(b"Service-ID: respmod-scan\r\n")
            .unwrap();
        options
            .parse_header_line(b"Max-Connections: 100\r\n")
            .unwrap();
        assert_eq!(options.server.as_deref(), Some("Example ICAP Server 1.0"));
        assert_eq!(options.service_tag, "\"W3E4R7U9-L3E4R7U9-W3E4R7U9\"");
        assert_eq!(options.service_id.as_deref(), Some("respmod-scan"));
        assert_eq!(options.max_connections, Some(100));
        options.check().unwrap();
    }

    #[test]
    fn parse_header_preview_rejects_invalid_value() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_header_line(b"Preview: not-a-number\r\n"),
            Err(IcapOptionsParseError::InvalidHeaderValue("Preview"))
        ));
    }

    #[test]
    fn parse_header_max_connections_rejects_invalid_value() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_header_line(b"Max-Connections: abc\r\n"),
            Err(IcapOptionsParseError::InvalidHeaderValue("Max-Connections"))
        ));
    }

    #[test]
    fn parse_header_encapsulated_rejects_unknown_part() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_header_line(b"Encapsulated: req-hdr=0\r\n"),
            Err(IcapOptionsParseError::InvalidHeaderValue("Encapsulated"))
        ));
    }

    #[test]
    fn parse_header_opt_body_type_ignored() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        options
            .parse_header_line(b"Opt-body-type: text/html\r\n")
            .unwrap();
    }

    #[test]
    fn parse_status_line_rejects_non_2xx() {
        let mut options = IcapServiceOptions::new(IcapMethod::Reqmod);
        assert!(matches!(
            options.parse_status_line(b"ICAP/1.0 500 Internal Error\r\n"),
            Err(IcapOptionsParseError::RequestFailed(500, _))
        ));
    }

    #[test]
    fn new_expired_is_expired() {
        let options = IcapServiceOptions::new_expired(IcapMethod::Options);
        assert!(options.expired());
    }

    #[tokio::test]
    async fn parse_full_options_response() {
        use std::io::Cursor;

        let data = b"ICAP/1.0 200 OK\r\n\
Methods: REQMOD\r\n\
Service: test\r\n\
ISTag: \"tag-1\"\r\n\
Allow: 204\r\n\
Preview: 1024\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        let options = IcapServiceOptions::parse(&mut reader, IcapMethod::Reqmod, 8192)
            .await
            .unwrap();
        assert!(options.support_204);
        assert!(!options.support_206);
        assert_eq!(options.preview_size, Some(1024));
        assert_eq!(options.service_tag, "\"tag-1\"");
        assert!(!options.expired());
    }

    #[tokio::test]
    async fn parse_full_options_requires_istag() {
        use std::io::Cursor;

        let data = b"ICAP/1.0 200 OK\r\n\
Methods: REQMOD\r\n\
\r\n";
        let mut reader = Cursor::new(&data[..]);
        match IcapServiceOptions::parse(&mut reader, IcapMethod::Reqmod, 8192).await {
            Err(IcapOptionsParseError::NoServiceTagSet) => {}
            Err(e) => panic!("unexpected error: {e}"),
            Ok(_) => panic!("expected NoServiceTagSet"),
        }
    }

    #[tokio::test]
    async fn parse_full_options_consumes_opt_body() {
        use std::io::Cursor;
        use tokio::io::AsyncReadExt;

        let data = b"ICAP/1.0 200 OK\r\n\
Methods: REQMOD\r\n\
ISTag: \"tag-1\"\r\n\
Encapsulated: opt-body=0\r\n\
Opt-body-type: text/html\r\n\
\r\n\
4\r\ntest\r\n0\r\n\r\nNEXT";
        let mut reader = Cursor::new(&data[..]);
        let options = IcapServiceOptions::parse(&mut reader, IcapMethod::Reqmod, 8192)
            .await
            .unwrap();
        assert_eq!(options.service_tag, "\"tag-1\"");
        assert!(options.has_opt_body);

        let mut leftover = Vec::new();
        reader.read_to_end(&mut leftover).await.unwrap();
        assert_eq!(leftover, b"NEXT");
    }
}
