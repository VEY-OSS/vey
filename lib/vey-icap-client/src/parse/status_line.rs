/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use atoi::FromRadix10;

use super::IcapLineParseError;

pub(crate) struct StatusLine<'a> {
    pub(crate) code: u16,
    pub(crate) message: &'a str,
}

impl<'a> StatusLine<'a> {
    pub(crate) fn parse(buf: &'a [u8]) -> Result<StatusLine<'a>, IcapLineParseError> {
        const PREFIX: &str = "ICAP/1.0 ";
        const MINIMAL_LENGTH: usize = 13; // ICAP/1.0 XYZ\n

        if buf.len() < MINIMAL_LENGTH {
            return Err(IcapLineParseError::NotLongEnough);
        }
        if !buf.starts_with(PREFIX.as_bytes()) {
            return Err(IcapLineParseError::InvalidIcapVersion);
        }

        let left = &buf[PREFIX.len()..];
        let (code, len) = u16::from_radix_10(left);
        if len != 3 || !(100..600).contains(&code) {
            return Err(IcapLineParseError::InvalidStatusCode);
        }

        if left.len() < len + 1 {
            return Err(IcapLineParseError::NotLongEnough);
        }
        let message = std::str::from_utf8(&left[len + 1..])?.trim();

        Ok(StatusLine { code, message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal() {
        let status = StatusLine::parse(b"ICAP/1.0 200 OK\r\n").unwrap();
        assert_eq!(status.code, 200);
        assert_eq!(status.message, "OK");
    }

    #[test]
    fn no_reason() {
        let status = StatusLine::parse(b"ICAP/1.0 200\r\n").unwrap();
        assert_eq!(status.code, 200);
        assert_eq!(status.message, "");
    }

    #[test]
    fn rejects_invalid_version() {
        match StatusLine::parse(b"HTTP/1.1 200 OK\r\n") {
            Err(IcapLineParseError::InvalidIcapVersion) => {}
            _ => panic!("expected InvalidIcapVersion"),
        }
    }

    #[test]
    fn rejects_invalid_status_code() {
        match StatusLine::parse(b"ICAP/1.0 99 Bad\r\n") {
            Err(IcapLineParseError::InvalidStatusCode) => {}
            _ => panic!("expected InvalidStatusCode"),
        }
    }

    #[test]
    fn rejects_too_short_input() {
        match StatusLine::parse(b"ICAP/1.0") {
            Err(IcapLineParseError::NotLongEnough) => {}
            _ => panic!("expected NotLongEnough"),
        }
    }

    #[test]
    fn accepts_1xx_and_5xx_codes() {
        let status = StatusLine::parse(b"ICAP/1.0 100 Continue\r\n").unwrap();
        assert_eq!(status.code, 100);
        assert_eq!(status.message, "Continue");

        let status = StatusLine::parse(b"ICAP/1.0 599 Custom\r\n").unwrap();
        assert_eq!(status.code, 599);
        assert_eq!(status.message, "Custom");
    }

    #[test]
    fn rejects_code_outside_valid_range() {
        match StatusLine::parse(b"ICAP/1.0 600 Too High\r\n") {
            Err(IcapLineParseError::InvalidStatusCode) => {}
            _ => panic!("expected InvalidStatusCode"),
        }
    }

    #[test]
    fn rejects_four_digit_status_code() {
        match StatusLine::parse(b"ICAP/1.0 2000 OK\r\n") {
            Err(IcapLineParseError::InvalidStatusCode) => {}
            _ => panic!("expected InvalidStatusCode"),
        }
    }

    #[test]
    fn rejects_invalid_utf8_message() {
        let mut buf = b"ICAP/1.0 200 ".to_vec();
        buf.push(0xff);
        buf.extend_from_slice(b"\r\n");
        match StatusLine::parse(&buf) {
            Err(IcapLineParseError::InvalidUtf8Encoding(_)) => {}
            _ => panic!("expected InvalidUtf8Encoding"),
        }
    }
}
