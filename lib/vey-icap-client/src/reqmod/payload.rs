/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use atoi::FromRadix10;

use super::IcapReqmodParseError;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum IcapReqmodResponsePayload {
    NoPayload,
    HttpRequestWithBody(usize),
    HttpRequestWithoutBody(usize),
    HttpResponseWithBody(usize),
    HttpResponseWithoutBody(usize),
}

impl IcapReqmodResponsePayload {
    pub(crate) fn parse(value: &str) -> Result<IcapReqmodResponsePayload, IcapReqmodParseError> {
        let mut parts = value.split(',');
        let hdr_part = parts
            .next()
            .ok_or(IcapReqmodParseError::InvalidHeaderValue("Encapsulated"))?
            .trim();

        let (name, value) = hdr_part
            .split_once('=')
            .ok_or(IcapReqmodParseError::InvalidHeaderValue("Encapsulated"))?;
        if value.ne("0") {
            return Err(IcapReqmodParseError::UnsupportedBody(
                "invalid hdr byte-offsets value",
            ));
        }

        match name.to_lowercase().as_str() {
            "req-hdr" => {
                let body_part = parts
                    .next()
                    .ok_or(IcapReqmodParseError::UnsupportedBody(
                        "no body byte-offsets pair found",
                    ))?
                    .trim();
                let (name, value) =
                    body_part
                        .split_once('=')
                        .ok_or(IcapReqmodParseError::UnsupportedBody(
                            "invalid body byte-offsets pair",
                        ))?;
                let (hdr_len, offset) = u32::from_radix_10(value.as_bytes());
                if offset != value.len() {
                    return Err(IcapReqmodParseError::UnsupportedBody(
                        "invalid body byte-offsets value",
                    ));
                }
                let hdr_len = hdr_len as usize;
                match name.to_lowercase().as_str() {
                    "req-body" => Ok(IcapReqmodResponsePayload::HttpRequestWithBody(hdr_len)),
                    "null-body" => Ok(IcapReqmodResponsePayload::HttpRequestWithoutBody(hdr_len)),
                    _ => Err(IcapReqmodParseError::UnsupportedBody(
                        "invalid body byte-offsets name",
                    )),
                }
            }
            "res-hdr" => {
                let body_part = parts
                    .next()
                    .ok_or(IcapReqmodParseError::UnsupportedBody(
                        "no body byte-offsets pair found",
                    ))?
                    .trim();
                let (name, value) =
                    body_part
                        .split_once('=')
                        .ok_or(IcapReqmodParseError::UnsupportedBody(
                            "invalid body byte-offsets pair",
                        ))?;
                let (hdr_len, offset) = u32::from_radix_10(value.as_bytes());
                if offset != value.len() {
                    return Err(IcapReqmodParseError::UnsupportedBody(
                        "invalid body byte-offsets value",
                    ));
                }
                let hdr_len = hdr_len as usize;
                match name.to_lowercase().as_str() {
                    "res-body" => Ok(IcapReqmodResponsePayload::HttpResponseWithBody(hdr_len)),
                    "null-body" => Ok(IcapReqmodResponsePayload::HttpResponseWithoutBody(hdr_len)),
                    _ => Err(IcapReqmodParseError::UnsupportedBody(
                        "invalid body byte-offsets name",
                    )),
                }
            }
            "null-body" => Ok(IcapReqmodResponsePayload::NoPayload),
            _ => Err(IcapReqmodParseError::UnsupportedBody(
                "invalid hdr byte-offsets value",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_null_body() {
        assert_eq!(
            IcapReqmodResponsePayload::parse("null-body=0").unwrap(),
            IcapReqmodResponsePayload::NoPayload
        );
    }

    #[test]
    fn parse_req_hdr_with_body() {
        assert_eq!(
            IcapReqmodResponsePayload::parse("req-hdr=0, req-body=128").unwrap(),
            IcapReqmodResponsePayload::HttpRequestWithBody(128)
        );
    }

    #[test]
    fn parse_req_hdr_without_body() {
        assert_eq!(
            IcapReqmodResponsePayload::parse("req-hdr=0, null-body=64").unwrap(),
            IcapReqmodResponsePayload::HttpRequestWithoutBody(64)
        );
    }

    #[test]
    fn parse_res_hdr_with_body() {
        assert_eq!(
            IcapReqmodResponsePayload::parse("res-hdr=0, res-body=256").unwrap(),
            IcapReqmodResponsePayload::HttpResponseWithBody(256)
        );
    }

    #[test]
    fn parse_res_hdr_without_body() {
        assert_eq!(
            IcapReqmodResponsePayload::parse("res-hdr=0, null-body=32").unwrap(),
            IcapReqmodResponsePayload::HttpResponseWithoutBody(32)
        );
    }

    #[test]
    fn rejects_non_zero_hdr_offset() {
        assert!(matches!(
            IcapReqmodResponsePayload::parse("req-hdr=8, req-body=16"),
            Err(IcapReqmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_missing_equals() {
        assert!(matches!(
            IcapReqmodResponsePayload::parse("null-body"),
            Err(IcapReqmodParseError::InvalidHeaderValue("Encapsulated"))
        ));
    }

    #[test]
    fn rejects_missing_body_part() {
        assert!(matches!(
            IcapReqmodResponsePayload::parse("req-hdr=0"),
            Err(IcapReqmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_invalid_body_name() {
        assert!(matches!(
            IcapReqmodResponsePayload::parse("req-hdr=0, opt-body=10"),
            Err(IcapReqmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_invalid_body_offset() {
        assert!(matches!(
            IcapReqmodResponsePayload::parse("req-hdr=0, req-body=abc"),
            Err(IcapReqmodParseError::UnsupportedBody(_))
        ));
    }
}
