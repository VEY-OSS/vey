/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;

use tokio::io::{AsyncRead, AsyncWrite};

use vey_io_ext::LimitedBufReadExt;

use super::FtpControlChannel;
use crate::error::FtpRawResponseError;

#[derive(Debug)]
pub(super) enum FtpRawResponse {
    SingleLine(u16, String),
    MultiLine(u16, Vec<String>),
}

impl FtpRawResponse {
    fn parse_code(byte: u8) -> Result<u16, FtpRawResponseError> {
        if byte.is_ascii_digit() {
            Ok((byte - b'0') as u16)
        } else {
            Err(FtpRawResponseError::InvalidLineFormat)
        }
    }

    fn parse_code_bytes(byte0: u8, byte1: u8, byte2: u8) -> Result<u16, FtpRawResponseError> {
        let code0 = Self::parse_code(byte0)?;
        let code1 = Self::parse_code(byte1)?;
        let code2 = Self::parse_code(byte2)?;
        let code = code0 * 100 + code1 * 10 + code2;
        if !(100..600).contains(&code) {
            Err(FtpRawResponseError::InvalidReplyCode(code))
        } else {
            Ok(code)
        }
    }

    fn parse_single_line(line: &str) -> Result<Self, FtpRawResponseError> {
        let buf = line.as_bytes();
        let code = Self::parse_code_bytes(buf[0], buf[1], buf[2])?;
        Ok(FtpRawResponse::SingleLine(code, line[4..].to_string()))
    }

    fn get_multi_line_parser(
        line: &str,
        max_lines: usize,
    ) -> Result<FtpMultiLineReplyParser, FtpRawResponseError> {
        let buf = line.as_bytes();
        let code = Self::parse_code_bytes(buf[0], buf[1], buf[2])?;
        let end_prefix = [buf[0], buf[1], buf[2], b' '];
        let mut lines = Vec::<String>::with_capacity(max_lines);
        lines.push(line[4..].to_string());
        Ok(FtpMultiLineReplyParser {
            code,
            end_prefix,
            lines,
        })
    }

    pub(super) fn code(&self) -> u16 {
        match self {
            FtpRawResponse::SingleLine(code, _) => *code,
            FtpRawResponse::MultiLine(code, _) => *code,
        }
    }

    pub(super) fn line_trimmed(&self) -> Option<&str> {
        match self {
            FtpRawResponse::SingleLine(_, line) => Some(line.as_str().trim()),
            FtpRawResponse::MultiLine(_, _) => None,
        }
    }

    pub(super) fn lines(&self) -> Option<&[String]> {
        match self {
            FtpRawResponse::SingleLine(_, _) => None,
            FtpRawResponse::MultiLine(_, lines) => Some(lines),
        }
    }

    pub(super) fn parse_pasv_227_reply(&self) -> Option<SocketAddr> {
        let line = match self {
            FtpRawResponse::SingleLine(_, line) => line,
            FtpRawResponse::MultiLine(_, _) => return None,
        };

        if let Some(p_start) = memchr::memchr(b'(', line.as_bytes())
            && let Some(p_end) = memchr::memchr(b')', &line.as_bytes()[p_start..])
        {
            let p_end = p_end + p_start;

            let a: Vec<&str> = line[p_start + 1..p_end].split(',').collect();
            if a.len() != 6 {
                return None;
            }

            let h1 = u8::from_str(a[0]).ok()?;
            let h2 = u8::from_str(a[1]).ok()?;
            let h3 = u8::from_str(a[2]).ok()?;
            let h4 = u8::from_str(a[3]).ok()?;
            let p1 = u8::from_str(a[4]).ok()?;
            let p2 = u8::from_str(a[5]).ok()?;

            let ip = IpAddr::V4(Ipv4Addr::new(h1, h2, h3, h4));
            let port = ((p1 as u16) << 8) + (p2 as u16);
            return Some(SocketAddr::new(ip, port));
        }

        None
    }

    pub(super) fn parse_epsv_229_reply(&self) -> Option<u16> {
        let line = match self {
            FtpRawResponse::SingleLine(_, line) => line,
            FtpRawResponse::MultiLine(_, _) => return None,
        };

        if let Some(p_start) = memchr::memchr(b'(', line.as_bytes())
            && let Some(p_end) = memchr::memchr(b')', &line.as_bytes()[p_start..])
        {
            let p_end = p_end + p_start;

            if !line[p_start + 1..p_end].starts_with("|||") {
                return None;
            }
            if p_end - 1 <= p_start + 4 {
                return None;
            }
            if line.as_bytes()[p_end - 1] != b'|' {
                return None;
            }
            let port = u16::from_str(&line[p_start + 4..p_end - 1]).ok()?;
            return Some(port);
        }

        None
    }

    pub(super) fn parse_spsv_227_reply(&self) -> Option<String> {
        let line = match self {
            FtpRawResponse::SingleLine(_, line) => line,
            FtpRawResponse::MultiLine(_, _) => return None,
        };

        if let Some(p_start) = memchr::memchr(b'(', line.as_bytes())
            && let Some(p_end) = memchr::memchr(b')', &line.as_bytes()[p_start..])
        {
            let identifier = line[p_start + 1..p_end].to_string();
            return Some(identifier);
        }
        // pure-ftpd has removed it's SPSV support in commit
        // https://github.com/jedisct1/pure-ftpd/commit/4828633d9cb42cd77d764e7d1cb3d0c04c5df001

        None
    }
}

pub(super) struct FtpMultiLineReplyParser {
    code: u16,
    end_prefix: [u8; 4],
    lines: Vec<String>,
}

impl FtpMultiLineReplyParser {
    pub(super) fn feed_line(&mut self, line: &str) -> Result<bool, FtpRawResponseError> {
        if line.as_bytes().starts_with(&self.end_prefix) {
            self.lines.push(line[4..].to_string());
            Ok(true)
        } else {
            // do not trim whitespace at beginning
            self.lines.push(line.to_string());
            Ok(false)
        }
    }

    pub(super) fn finish(self) -> FtpRawResponse {
        FtpRawResponse::MultiLine(self.code, self.lines)
    }
}

impl<T> FtpControlChannel<T>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    async fn read_first_line<'a>(
        &mut self,
        buf: &'a mut Vec<u8>,
    ) -> Result<&'a str, FtpRawResponseError> {
        buf.clear();

        let (found, len) = self
            .stream
            .limited_read_until(b'\n', self.config.max_line_len, buf)
            .await
            .map_err(FtpRawResponseError::ReadFailed)?;
        match len {
            0 => Err(FtpRawResponseError::ConnectionClosed),
            1..=4 => {
                // at least <code>\n
                match std::str::from_utf8(buf) {
                    Ok(rsp) => {
                        crate::log_rsp!(rsp.trim_end());
                        Err(FtpRawResponseError::InvalidLineFormat)
                    }
                    Err(_) => Err(FtpRawResponseError::LineIsNotUtf8),
                }
            }
            _ => {
                let rsp = std::str::from_utf8(buf)
                    .map_err(|_| FtpRawResponseError::LineIsNotUtf8)?
                    .trim_end();
                crate::log_rsp!(rsp);

                if !found {
                    Err(FtpRawResponseError::LineTooLong)
                } else {
                    Ok(rsp)
                }
            }
        }
    }

    async fn read_extra_line<'a>(
        &mut self,
        buf: &'a mut Vec<u8>,
    ) -> Result<&'a str, FtpRawResponseError> {
        buf.clear();

        let (found, len) = self
            .stream
            .limited_read_until(b'\n', self.config.max_line_len, buf)
            .await
            .map_err(FtpRawResponseError::ReadFailed)?;
        match len {
            0 => Err(FtpRawResponseError::ConnectionClosed),
            1 => {
                // at least "\n"
                match std::str::from_utf8(buf) {
                    Ok(rsp) => {
                        crate::log_rsp!(rsp.trim_end());
                        Err(FtpRawResponseError::InvalidLineFormat)
                    }
                    Err(_) => Err(FtpRawResponseError::LineIsNotUtf8),
                }
            }
            _ => {
                let rsp = std::str::from_utf8(buf)
                    .map_err(|_| FtpRawResponseError::LineIsNotUtf8)?
                    .trim_end();
                crate::log_rsp!(rsp);

                if !found {
                    Err(FtpRawResponseError::LineTooLong)
                } else {
                    Ok(rsp)
                }
            }
        }
    }

    pub(super) async fn read_raw_response(
        &mut self,
    ) -> Result<FtpRawResponse, FtpRawResponseError> {
        let mut buf = Vec::<u8>::with_capacity(self.config.max_line_len);
        let line = self.read_first_line(&mut buf).await?;

        match line.as_bytes()[3] {
            b' ' => FtpRawResponse::parse_single_line(line),
            b'-' => {
                let mut ml_parser =
                    FtpRawResponse::get_multi_line_parser(line, self.config.max_multi_lines)?;
                for _i in 0..self.config.max_multi_lines {
                    let line = self.read_extra_line(&mut buf).await?;
                    let end = ml_parser.feed_line(line)?;
                    if end {
                        return Ok(ml_parser.finish());
                    }
                }
                Err(FtpRawResponseError::TooManyLines)
            }
            _ => Err(FtpRawResponseError::InvalidLineFormat),
        }
    }

    pub(super) async fn timed_read_raw_response(
        &mut self,
        stage: &'static str,
    ) -> Result<FtpRawResponse, FtpRawResponseError> {
        match tokio::time::timeout(self.config.command_timeout, self.read_raw_response()).await {
            Ok(r) => r,
            Err(_) => Err(FtpRawResponseError::ReadResponseTimedOut(stage)),
        }
    }
}
