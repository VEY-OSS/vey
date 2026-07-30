/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use atoi::FromRadix10;

use super::IcapRespmodParseError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IcapRespmodResponsePayload {
    NoPayload,
    HttpResponseWithBody(usize),
    HttpResponseWithoutBody(usize),
}

impl IcapRespmodResponsePayload {
    pub(crate) fn parse(value: &str) -> Result<IcapRespmodResponsePayload, IcapRespmodParseError> {
        let mut parts = value.split(',');
        let hdr_part = parts
            .next()
            .ok_or(IcapRespmodParseError::InvalidHeaderValue("Encapsulated"))?
            .trim();

        let (name, value) = hdr_part
            .split_once('=')
            .ok_or(IcapRespmodParseError::InvalidHeaderValue("Encapsulated"))?;
        if value.ne("0") {
            return Err(IcapRespmodParseError::UnsupportedBody(
                "invalid hdr byte-offsets value",
            ));
        }

        match name.to_lowercase().as_str() {
            "res-hdr" => {
                let body_part = parts
                    .next()
                    .ok_or(IcapRespmodParseError::UnsupportedBody(
                        "no body byte-offsets pair found",
                    ))?
                    .trim();
                let (name, value) =
                    body_part
                        .split_once('=')
                        .ok_or(IcapRespmodParseError::UnsupportedBody(
                            "invalid body byte-offsets pair",
                        ))?;
                let (hdr_len, offset) = u32::from_radix_10(value.as_bytes());
                if offset != value.len() {
                    return Err(IcapRespmodParseError::UnsupportedBody(
                        "invalid body byte-offsets value",
                    ));
                }
                let hdr_len = hdr_len as usize;
                match name.to_lowercase().as_str() {
                    "res-body" => Ok(IcapRespmodResponsePayload::HttpResponseWithBody(hdr_len)),
                    "null-body" => Ok(IcapRespmodResponsePayload::HttpResponseWithoutBody(hdr_len)),
                    _ => Err(IcapRespmodParseError::UnsupportedBody(
                        "invalid body byte-offsets name",
                    )),
                }
            }
            "null-body" => Ok(IcapRespmodResponsePayload::NoPayload),
            _ => Err(IcapRespmodParseError::UnsupportedBody(
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
            IcapRespmodResponsePayload::parse("null-body=0").unwrap(),
            IcapRespmodResponsePayload::NoPayload
        );
    }

    #[test]
    fn parse_res_hdr_with_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("res-hdr=0, res-body=128").unwrap(),
            IcapRespmodResponsePayload::HttpResponseWithBody(128)
        );
    }

    #[test]
    fn parse_res_hdr_without_body() {
        assert_eq!(
            IcapRespmodResponsePayload::parse("res-hdr=0, null-body=64").unwrap(),
            IcapRespmodResponsePayload::HttpResponseWithoutBody(64)
        );
    }

    #[test]
    fn rejects_req_hdr() {
        assert!(matches!(
            IcapRespmodResponsePayload::parse("req-hdr=0, req-body=16"),
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
    }
}
