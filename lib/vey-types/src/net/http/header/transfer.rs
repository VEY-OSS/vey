/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;

use thiserror::Error;

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransferCompressKind {
    #[default]
    Identity,
    Compress,
    Deflate,
    Gzip,
}

impl TransferCompressKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Compress => "compress",
            Self::Deflate => "deflate",
            Self::Gzip => "gzip",
        }
    }
}

impl fmt::Display for TransferCompressKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Error, Debug)]
pub enum InvalidTransferEncodingValue {
    #[error("compress kind already set to {0}")]
    CompressKindAlreadySet(TransferCompressKind),
    #[error("invalid chunked position")]
    InvalidChunkedPosition,
    #[error("invalid coding type")]
    InvalidCodingType,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferEncodingValue {
    compress_kind: Option<TransferCompressKind>,
    chunked: bool,
}

impl TransferEncodingValue {
    #[inline]
    pub fn chunked(&self) -> bool {
        self.chunked
    }

    #[inline]
    pub fn compress_kind(&self) -> TransferCompressKind {
        self.compress_kind.unwrap_or(TransferCompressKind::Identity)
    }

    fn set_compress_kind(
        &mut self,
        kind: TransferCompressKind,
    ) -> Result<(), InvalidTransferEncodingValue> {
        if let Some(kind) = self.compress_kind {
            return Err(InvalidTransferEncodingValue::CompressKindAlreadySet(kind));
        }
        self.compress_kind = Some(kind);
        Ok(())
    }

    pub fn parse(&mut self, buf: &[u8]) -> Result<(), InvalidTransferEncodingValue> {
        let mut left = buf;
        while !left.is_empty() {
            if self.chunked {
                return Err(InvalidTransferEncodingValue::InvalidChunkedPosition);
            }

            let this = match memchr::memchr(b',', left) {
                Some(p) => {
                    let this = &left[..p];
                    left = &left[p + 1..];
                    this
                }
                None => {
                    let this = left;
                    left = &[];
                    this
                }
            };

            let kind = match memchr::memchr(b';', this) {
                Some(p) => &this[..p],
                None => this,
            };

            let kind = kind.trim_ascii();
            if kind.is_empty() {
                continue;
            }
            match kind[0] {
                b'C' | b'c' => {
                    if kind.eq_ignore_ascii_case(b"chunked") {
                        self.chunked = true;
                        continue;
                    }
                    if kind.eq_ignore_ascii_case(b"compress") {
                        self.set_compress_kind(TransferCompressKind::Compress)?;
                        continue;
                    }
                }
                b'D' | b'd' => {
                    if kind.eq_ignore_ascii_case(b"deflate") {
                        self.set_compress_kind(TransferCompressKind::Deflate)?;
                        continue;
                    }
                }
                b'G' | b'g' => {
                    if kind.eq_ignore_ascii_case(b"gzip") {
                        self.set_compress_kind(TransferCompressKind::Gzip)?;
                        continue;
                    }
                }
                b'I' | b'i' => {
                    if kind.eq_ignore_ascii_case(b"identity") {
                        self.set_compress_kind(TransferCompressKind::Identity)?;
                        continue;
                    }
                }
                b'X' | b'x' => {
                    if kind.eq_ignore_ascii_case(b"x-gzip") {
                        self.set_compress_kind(TransferCompressKind::Gzip)?;
                        continue;
                    }
                    if kind.eq_ignore_ascii_case(b"x-compress") {
                        self.set_compress_kind(TransferCompressKind::Compress)?;
                        continue;
                    }
                }
                _ => {}
            }
            return Err(InvalidTransferEncodingValue::InvalidCodingType);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<TransferEncodingValue, InvalidTransferEncodingValue> {
        let mut v = TransferEncodingValue::default();
        v.parse(s.as_bytes())?;
        Ok(v)
    }

    #[test]
    fn default_is_identity_not_chunked() {
        let v = TransferEncodingValue::default();
        assert!(!v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Identity);
    }

    #[test]
    fn last_coding_chunked() {
        let v = parse("chunked").unwrap();
        assert!(v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Identity);

        let v = parse("gzip, chunked").unwrap();
        assert!(v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Gzip);

        let v = parse(" gzip , CHUNKED ").unwrap();
        assert!(v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Gzip);

        let v = parse("gzip;q=1.0, chunked").unwrap();
        assert!(v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Gzip);

        let v = parse("chunked,").unwrap();
        assert!(v.chunked());
    }

    #[test]
    fn compress_codings() {
        assert_eq!(
            parse("gzip").unwrap().compress_kind(),
            TransferCompressKind::Gzip
        );
        assert_eq!(
            parse("deflate").unwrap().compress_kind(),
            TransferCompressKind::Deflate
        );
        assert_eq!(
            parse("compress").unwrap().compress_kind(),
            TransferCompressKind::Compress
        );
        assert_eq!(
            parse("identity").unwrap().compress_kind(),
            TransferCompressKind::Identity
        );
        assert_eq!(
            parse("x-gzip").unwrap().compress_kind(),
            TransferCompressKind::Gzip
        );
        assert_eq!(
            parse("x-compress").unwrap().compress_kind(),
            TransferCompressKind::Compress
        );
        assert!(!parse("gzip").unwrap().chunked());
    }

    #[test]
    fn empty_tokens_are_skipped() {
        let v = parse(",,chunked").unwrap();
        assert!(v.chunked());
        assert!(parse("").unwrap() == TransferEncodingValue::default());
        assert!(parse(" , , ").unwrap() == TransferEncodingValue::default());
    }

    #[test]
    fn suffix_lookalikes_are_invalid() {
        assert!(matches!(
            parse("notchunked"),
            Err(InvalidTransferEncodingValue::InvalidCodingType)
        ));
        assert!(matches!(
            parse("foochunked"),
            Err(InvalidTransferEncodingValue::InvalidCodingType)
        ));
        assert!(matches!(
            parse("br"),
            Err(InvalidTransferEncodingValue::InvalidCodingType)
        ));
    }

    #[test]
    fn chunked_must_be_last() {
        assert!(matches!(
            parse("chunked, gzip"),
            Err(InvalidTransferEncodingValue::InvalidChunkedPosition)
        ));
        assert!(matches!(
            parse("gzip, chunked, deflate"),
            Err(InvalidTransferEncodingValue::InvalidChunkedPosition)
        ));
        assert!(matches!(
            parse("chunked, chunked"),
            Err(InvalidTransferEncodingValue::InvalidChunkedPosition)
        ));
    }

    #[test]
    fn only_one_compress_kind() {
        match parse("gzip, deflate") {
            Err(InvalidTransferEncodingValue::CompressKindAlreadySet(kind)) => {
                assert_eq!(kind, TransferCompressKind::Gzip);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn incremental_headers() {
        let mut v = TransferEncodingValue::default();
        v.parse(b"gzip").unwrap();
        v.parse(b"chunked").unwrap();
        assert!(v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Gzip);

        let mut v = TransferEncodingValue::default();
        v.parse(b"chunked").unwrap();
        assert!(matches!(
            v.parse(b"gzip"),
            Err(InvalidTransferEncodingValue::InvalidChunkedPosition)
        ));
    }

    #[test]
    fn compress_kind_display() {
        assert_eq!(TransferCompressKind::Gzip.as_str(), "gzip");
        assert_eq!(TransferCompressKind::Identity.to_string(), "identity");
        assert_eq!(TransferCompressKind::Compress.to_string(), "compress");
        assert_eq!(TransferCompressKind::Deflate.to_string(), "deflate");
    }
}
