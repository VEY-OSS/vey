/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

/// How a `Transfer-Encoding` field should drive body framing (RFC 9112).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferEncodingKind {
    /// The last transfer-coding is `chunked`.
    Chunked,
    /// TE is present and well-formed, but the last coding is not `chunked`.
    Other,
}

impl TransferEncodingKind {
    /// Parse a `Transfer-Encoding` field value.
    ///
    /// Codings are comma-separated and compared case-insensitively. Optional
    /// parameters after `;` are ignored. `chunked` is only valid as the final
    /// coding, and MUST NOT be applied more than once.
    ///
    /// Returns `None` for an empty list, `chunked` not last, or duplicate `chunked`.
    pub fn parse(value: &str) -> Option<Self> {
        let mut last_is_chunked = false;
        let mut any = false;

        for raw in value.split(',') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let coding = match token.split_once(';') {
                Some((name, _)) => name.trim(),
                None => token,
            };
            if coding.is_empty() {
                continue;
            }
            if last_is_chunked {
                return None;
            }
            any = true;
            last_is_chunked = coding.eq_ignore_ascii_case("chunked");
        }

        if !any {
            return None;
        }
        Some(if last_is_chunked {
            Self::Chunked
        } else {
            Self::Other
        })
    }
}

pub fn transfer_encoding_chunked() -> &'static str {
    "Transfer-Encoding: chunked\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_coding_chunked_is_accepted() {
        assert_eq!(
            TransferEncodingKind::parse("chunked"),
            Some(TransferEncodingKind::Chunked)
        );
        assert_eq!(
            TransferEncodingKind::parse("gzip, chunked"),
            Some(TransferEncodingKind::Chunked)
        );
        assert_eq!(
            TransferEncodingKind::parse(" gzip , CHUNKED "),
            Some(TransferEncodingKind::Chunked)
        );
        assert_eq!(
            TransferEncodingKind::parse("gzip;q=1.0, chunked"),
            Some(TransferEncodingKind::Chunked)
        );
    }

    #[test]
    fn suffix_lookalikes_are_not_chunked() {
        assert_eq!(
            TransferEncodingKind::parse("notchunked"),
            Some(TransferEncodingKind::Other)
        );
        assert_eq!(
            TransferEncodingKind::parse("foochunked"),
            Some(TransferEncodingKind::Other)
        );
        assert_eq!(
            TransferEncodingKind::parse("gzip"),
            Some(TransferEncodingKind::Other)
        );
    }

    #[test]
    fn chunked_not_last_or_empty_is_invalid() {
        assert_eq!(TransferEncodingKind::parse("chunked, gzip"), None);
        assert_eq!(TransferEncodingKind::parse("gzip, chunked, deflate"), None);
        assert_eq!(TransferEncodingKind::parse("chunked, chunked"), None);
        assert_eq!(TransferEncodingKind::parse(""), None);
        assert_eq!(TransferEncodingKind::parse(" , , "), None);
    }

    #[test]
    fn chunked_header_line() {
        assert_eq!(
            transfer_encoding_chunked(),
            "Transfer-Encoding: chunked\r\n"
        );
    }
}
