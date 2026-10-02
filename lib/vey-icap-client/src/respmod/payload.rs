/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::IcapRespmodParseError;
use crate::parse::encapsulated::parse_offset;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcapRespmodResponsePayload {
    NoPayload,
    HttpResponseWithBody(usize),
    HttpResponseWithoutBody(usize),
}

impl IcapRespmodResponsePayload {
    /// Parse the `Encapsulated` header value.
    ///
    /// Also return the size of the http request header which is placed before
    /// the http response header, and which should be skipped.
    pub(crate) fn parse(
        value: &str,
    ) -> Result<(IcapRespmodResponsePayload, usize), IcapRespmodParseError> {
        let mut parts = value.split(',').map(str::trim);
        let (name, value) = parts
            .next()
            .and_then(|p| p.split_once('='))
            .ok_or(IcapRespmodParseError::InvalidHeaderValue("Encapsulated"))?;
        if value.ne("0") {
            return Err(IcapRespmodParseError::UnsupportedBody(
                "invalid hdr byte-offsets value",
            ));
        }

        let res_hdr_offset = match name.to_lowercase().as_str() {
            "null-body" => return Ok((IcapRespmodResponsePayload::NoPayload, 0)),
            "res-hdr" => 0,
            "req-hdr" => {
                let (name, value) = parts.next().and_then(|p| p.split_once('=')).ok_or(
                    IcapRespmodParseError::UnsupportedBody("no res-hdr byte-offsets pair found"),
                )?;
                if !name.eq_ignore_ascii_case("res-hdr") {
                    return Err(IcapRespmodParseError::UnsupportedBody(
                        "invalid res-hdr byte-offsets name",
                    ));
                }
                parse_offset(value).ok_or(IcapRespmodParseError::UnsupportedBody(
                    "invalid res-hdr byte-offsets value",
                ))?
            }
            _ => {
                return Err(IcapRespmodParseError::UnsupportedBody(
                    "invalid hdr byte-offsets name",
                ));
            }
        };

        let (name, value) = parts
            .next()
            .ok_or(IcapRespmodParseError::UnsupportedBody(
                "no body byte-offsets pair found",
            ))?
            .split_once('=')
            .ok_or(IcapRespmodParseError::UnsupportedBody(
                "invalid body byte-offsets pair",
            ))?;
        let hdr_len = parse_offset(value)
            .and_then(|body_offset| body_offset.checked_sub(res_hdr_offset))
            .filter(|n| *n > 0)
            .ok_or(IcapRespmodParseError::UnsupportedBody(
                "invalid body byte-offsets value",
            ))?;
        let payload = match name.to_lowercase().as_str() {
            "res-body" => IcapRespmodResponsePayload::HttpResponseWithBody(hdr_len),
            "null-body" => IcapRespmodResponsePayload::HttpResponseWithoutBody(hdr_len),
            _ => {
                return Err(IcapRespmodParseError::UnsupportedBody(
                    "invalid body byte-offsets name",
                ));
            }
        };
        Ok((payload, res_hdr_offset))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_null_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("null-body=0").unwrap(),
            (IcapRespmodResponsePayload::NoPayload, 0)
        );
    }

    #[test]
    fn parse_res_hdr_with_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("res-hdr=0, res-body=128").unwrap(),
            (IcapRespmodResponsePayload::HttpResponseWithBody(128), 0)
        );
    }

    #[test]
    fn parse_res_hdr_without_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("res-hdr=0, null-body=64").unwrap(),
            (IcapRespmodResponsePayload::HttpResponseWithoutBody(64), 0)
        );
    }

    #[test]
    fn parse_req_hdr_and_res_hdr_with_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("req-hdr=0, res-hdr=100, res-body=228").unwrap(),
            (IcapRespmodResponsePayload::HttpResponseWithBody(128), 100)
        );
    }

    #[test]
    fn parse_req_hdr_and_res_hdr_without_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("REQ-HDR=0, Res-Hdr=10, null-body=74").unwrap(),
            (IcapRespmodResponsePayload::HttpResponseWithoutBody(64), 10)
        );
    }

    #[test]
    fn rejects_req_hdr_without_res_hdr() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("req-hdr=0, req-body=16"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
        assert!(matches!(
            IcapRespmodResponsePayload::parse("req-hdr=0, null-body=16"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_body_offset_not_after_res_hdr() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("req-hdr=0, res-hdr=100, res-body=100"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
        assert!(matches!(
            IcapRespmodResponsePayload::parse("req-hdr=0, res-hdr=100, res-body=50"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_non_zero_hdr_offset() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("res-hdr=8, res-body=16"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_missing_equals() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("null-body"),
            Err(IcapRespmodParseError::InvalidHeaderValue("Encapsulated"))
        ));
    }

    #[test]
    fn rejects_missing_body_part() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("res-hdr=0"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
    }

    #[test]
    fn rejects_invalid_body_offset() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("res-hdr=0, res-body=1x"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
        assert!(matches!(
            IcapRespmodResponsePayload::parse("res-hdr=0, res-body="),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
        assert!(matches!(
            IcapRespmodResponsePayload::parse("res-hdr=0, res-body=0"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
        assert!(matches!(
            IcapRespmodResponsePayload::parse("res-hdr=0, res-body=99999999999999999999999"),
            Err(IcapRespmodParseError::UnsupportedBody(_))
        ));
    }
}
