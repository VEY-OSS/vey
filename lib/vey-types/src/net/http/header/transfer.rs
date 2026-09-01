/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;

use bytes::BufMut;
use thiserror::Error;

use super::HttpStructuredFieldParser;

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

/// HTTP `qvalue` / rank (`0` ..= `1`, at most 3 decimal digits), stored as thousandths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferCodingQValue(u16);

impl TransferCodingQValue {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1000);

    #[inline]
    pub fn thousandths(self) -> u16 {
        self.0
    }

    fn parse(buf: &[u8]) -> Result<Self, InvalidAcceptTransferEncodingValue> {
        if buf.is_empty() {
            return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
        }
        match buf[0] {
            b'1' => {
                let rest = &buf[1..];
                if rest.is_empty() {
                    return Ok(Self::ONE);
                }
                if rest[0] != b'.' || rest.len() > 4 {
                    return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
                }
                if rest[1..].iter().any(|&b| b != b'0') {
                    return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
                }
                Ok(Self::ONE)
            }
            b'0' => {
                let rest = &buf[1..];
                if rest.is_empty() {
                    return Ok(Self::ZERO);
                }
                if rest[0] != b'.' || rest.len() > 4 {
                    return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
                }
                let digits = &rest[1..];
                let mut v = 0u16;
                for (i, &b) in digits.iter().enumerate() {
                    if !b.is_ascii_digit() {
                        return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
                    }
                    v += (b - b'0') as u16 * 10u16.pow(2 - i as u32);
                }
                Ok(Self(v))
            }
            _ => Err(InvalidAcceptTransferEncodingValue::InvalidQValue),
        }
    }
}

impl Default for TransferCodingQValue {
    fn default() -> Self {
        Self::ONE
    }
}

impl fmt::Display for TransferCodingQValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            1000 => f.write_str("1"),
            0 => f.write_str("0"),
            v => {
                let a = v / 100;
                let b = (v / 10) % 10;
                let c = v % 10;
                if c != 0 {
                    write!(f, "0.{a}{b}{c}")
                } else if b != 0 {
                    write!(f, "0.{a}{b}")
                } else {
                    write!(f, "0.{a}")
                }
            }
        }
    }
}

enum TransferCodingName {
    Chunked,
    Trailers,
    Compress(TransferCompressKind),
}

fn parse_transfer_coding_name(kind: &[u8]) -> Option<TransferCodingName> {
    if kind.is_empty() {
        return None;
    }
    match kind[0] {
        b'C' | b'c' => {
            if kind.eq_ignore_ascii_case(b"chunked") {
                return Some(TransferCodingName::Chunked);
            }
            if kind.eq_ignore_ascii_case(b"compress") {
                return Some(TransferCodingName::Compress(TransferCompressKind::Compress));
            }
        }
        b'D' | b'd' => {
            if kind.eq_ignore_ascii_case(b"deflate") {
                return Some(TransferCodingName::Compress(TransferCompressKind::Deflate));
            }
        }
        b'G' | b'g' => {
            if kind.eq_ignore_ascii_case(b"gzip") {
                return Some(TransferCodingName::Compress(TransferCompressKind::Gzip));
            }
        }
        b'I' | b'i' => {
            if kind.eq_ignore_ascii_case(b"identity") {
                return Some(TransferCodingName::Compress(TransferCompressKind::Identity));
            }
        }
        b'T' | b't' => {
            if kind.eq_ignore_ascii_case(b"trailers") {
                return Some(TransferCodingName::Trailers);
            }
        }
        b'X' | b'x' => {
            if kind.eq_ignore_ascii_case(b"x-gzip") {
                return Some(TransferCodingName::Compress(TransferCompressKind::Gzip));
            }
            if kind.eq_ignore_ascii_case(b"x-compress") {
                return Some(TransferCodingName::Compress(TransferCompressKind::Compress));
            }
        }
        _ => {}
    }
    None
}

fn parse_q_param(
    params: &[u8],
) -> Result<TransferCodingQValue, InvalidAcceptTransferEncodingValue> {
    let params = params.trim_ascii();
    if params.len() < 2 || !params[..1].eq_ignore_ascii_case(b"q") || params[1] != b'=' {
        return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
    }
    if memchr::memchr(b';', params).is_some() {
        return Err(InvalidAcceptTransferEncodingValue::InvalidQValue);
    }
    TransferCodingQValue::parse(&params[2..])
}

#[derive(Error, Debug)]
pub enum InvalidTransferEncodingValue {
    #[error("compress kind already set to {0}")]
    CompressKindAlreadySet(TransferCompressKind),
    #[error("invalid chunked position")]
    InvalidChunkedPosition,
    #[error("invalid coding type")]
    InvalidCodingType,
    #[error("unexpected transfer-coding parameter")]
    UnexpectedParameter,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransferEncodingValue {
    compress_kind: Option<TransferCompressKind>,
    chunked: bool,
}

impl TransferEncodingValue {
    pub const CHUNKED: Self = Self {
        compress_kind: None,
        chunked: true,
    };

    #[inline]
    pub fn chunked(&self) -> bool {
        self.chunked
    }

    #[inline]
    pub fn compress_kind(&self) -> TransferCompressKind {
        self.compress_kind.unwrap_or(TransferCompressKind::Identity)
    }

    pub fn body_compressed(&self) -> bool {
        self.compress_kind
            .map(|kind| !matches!(kind, TransferCompressKind::Identity))
            .unwrap_or(false)
    }

    pub fn write_chunked(&self, original_name: &[u8], buf: &mut Vec<u8>) {
        if !self.chunked {
            return;
        }
        buf.put_slice(original_name);
        buf.put_slice(b": chunked\r\n");
    }

    pub fn write(&self, original_name: &[u8], buf: &mut Vec<u8>) {
        if self.compress_kind.is_none() && !self.chunked {
            return;
        }
        buf.put_slice(original_name);
        buf.put_slice(b": ");
        if let Some(kind) = self.compress_kind {
            buf.put_slice(kind.as_str().as_bytes());
            if self.chunked {
                buf.put_slice(b", chunked");
            }
        } else {
            buf.put_slice(b"chunked");
        }
        buf.put_slice(b"\r\n");
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
        for item in buf.as_item_list() {
            if self.chunked {
                return Err(InvalidTransferEncodingValue::InvalidChunkedPosition);
            }

            if !item.params().is_empty() {
                return Err(InvalidTransferEncodingValue::UnexpectedParameter);
            }
            match parse_transfer_coding_name(item.value()) {
                Some(TransferCodingName::Chunked) => {
                    self.chunked = true;
                }
                Some(TransferCodingName::Compress(kind)) => {
                    self.set_compress_kind(kind)?;
                }
                Some(TransferCodingName::Trailers) | None => {
                    return Err(InvalidTransferEncodingValue::InvalidCodingType);
                }
            }
        }

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum InvalidAcceptTransferEncodingValue {
    #[error("compress kind already set to {0}")]
    CompressKindAlreadySet(TransferCompressKind),
    #[error("invalid coding type")]
    InvalidCodingType,
    #[error("invalid qvalue")]
    InvalidQValue,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AcceptTransferEncodingValue {
    trailers: bool,
    gzip: Option<TransferCodingQValue>,
    deflate: Option<TransferCodingQValue>,
    compress: Option<TransferCodingQValue>,
    identity: Option<TransferCodingQValue>,
}

impl AcceptTransferEncodingValue {
    #[inline]
    pub fn trailers(&self) -> bool {
        self.trailers
    }

    pub fn qvalue(&self, kind: TransferCompressKind) -> Option<TransferCodingQValue> {
        match kind {
            TransferCompressKind::Gzip => self.gzip,
            TransferCompressKind::Deflate => self.deflate,
            TransferCompressKind::Compress => self.compress,
            TransferCompressKind::Identity => self.identity,
        }
    }

    pub fn write_trailers(&self, original_name: &[u8], buf: &mut Vec<u8>) {
        if !self.trailers {
            return;
        }
        buf.put_slice(original_name);
        buf.put_slice(b": trailers\r\n");
    }

    fn set_qvalue(
        &mut self,
        kind: TransferCompressKind,
        q: TransferCodingQValue,
    ) -> Result<(), InvalidAcceptTransferEncodingValue> {
        let slot = match kind {
            TransferCompressKind::Gzip => &mut self.gzip,
            TransferCompressKind::Deflate => &mut self.deflate,
            TransferCompressKind::Compress => &mut self.compress,
            TransferCompressKind::Identity => &mut self.identity,
        };
        if slot.is_some() {
            return Err(InvalidAcceptTransferEncodingValue::CompressKindAlreadySet(
                kind,
            ));
        }
        *slot = Some(q);
        Ok(())
    }

    pub fn parse(&mut self, buf: &[u8]) -> Result<(), InvalidAcceptTransferEncodingValue> {
        for item in buf.as_item_list() {
            let q = if item.params().is_empty() {
                TransferCodingQValue::ONE
            } else {
                parse_q_param(item.params())?
            };
            match parse_transfer_coding_name(item.value()) {
                Some(TransferCodingName::Trailers) => {
                    self.trailers = true;
                }
                Some(TransferCodingName::Compress(kind)) => {
                    self.set_qvalue(kind, q)?;
                }
                Some(TransferCodingName::Chunked) | None => {
                    return Err(InvalidAcceptTransferEncodingValue::InvalidCodingType);
                }
            }
        }
        Ok(())
    }
}

impl fmt::Display for AcceptTransferEncodingValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for kind in [
            TransferCompressKind::Gzip,
            TransferCompressKind::Deflate,
            TransferCompressKind::Compress,
            TransferCompressKind::Identity,
        ] {
            let Some(q) = self.qvalue(kind) else {
                continue;
            };
            if !first {
                f.write_str(", ")?;
            }
            first = false;
            f.write_str(kind.as_str())?;
            if q != TransferCodingQValue::ONE {
                write!(f, ";q={q}")?;
            }
        }
        if self.trailers {
            if !first {
                f.write_str(", ")?;
            }
            f.write_str("trailers")?;
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

    fn parse_te(
        s: &str,
    ) -> Result<AcceptTransferEncodingValue, InvalidAcceptTransferEncodingValue> {
        let mut v = AcceptTransferEncodingValue::default();
        v.parse(s.as_bytes())?;
        Ok(v)
    }

    #[test]
    fn default_is_identity_not_chunked() {
        let v = TransferEncodingValue::default();
        assert!(!v.chunked());
        assert_eq!(v.compress_kind(), TransferCompressKind::Identity);
        assert!(TransferEncodingValue::CHUNKED.chunked());
        assert!(!TransferEncodingValue::CHUNKED.body_compressed());
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
        assert!(matches!(
            parse("gzip;q=1.0, chunked"),
            Err(InvalidTransferEncodingValue::UnexpectedParameter)
        ));
        assert!(matches!(
            parse("chunked;foo=bar"),
            Err(InvalidTransferEncodingValue::UnexpectedParameter)
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
        assert!(!parse("chunked").unwrap().body_compressed());
        assert!(!parse("identity, chunked").unwrap().body_compressed());
        assert!(parse("gzip, chunked").unwrap().body_compressed());
    }

    #[test]
    fn te_parses_codings_and_trailers() {
        let v = parse_te("gzip, trailers;q=1.0").unwrap();
        assert!(v.trailers());
        assert_eq!(
            v.qvalue(TransferCompressKind::Gzip),
            Some(TransferCodingQValue::ONE)
        );
        assert_eq!(v.qvalue(TransferCompressKind::Deflate), None);

        let v = parse_te("deflate;q=0.5, gzip;q=1, TRAILERS").unwrap();
        assert!(v.trailers());
        assert_eq!(
            v.qvalue(TransferCompressKind::Gzip),
            Some(TransferCodingQValue::ONE)
        );
        assert_eq!(
            v.qvalue(TransferCompressKind::Deflate)
                .map(|q| q.thousandths()),
            Some(500)
        );

        let v = parse_te(" x-gzip ;q=0.8 , identity;q=0 ").unwrap();
        assert!(!v.trailers());
        assert_eq!(
            v.qvalue(TransferCompressKind::Gzip)
                .map(|q| q.thousandths()),
            Some(800)
        );
        assert_eq!(
            v.qvalue(TransferCompressKind::Identity),
            Some(TransferCodingQValue::ZERO)
        );

        assert!(parse_te("").unwrap() == AcceptTransferEncodingValue::default());
        assert!(!parse_te("deflate").unwrap().trailers());
    }

    #[test]
    fn te_rejects_invalid() {
        assert!(matches!(
            parse_te("nottrailers"),
            Err(InvalidAcceptTransferEncodingValue::InvalidCodingType)
        ));
        assert!(matches!(
            parse_te("chunked"),
            Err(InvalidAcceptTransferEncodingValue::InvalidCodingType)
        ));
        assert!(matches!(
            parse_te("gzip, gzip"),
            Err(InvalidAcceptTransferEncodingValue::CompressKindAlreadySet(
                TransferCompressKind::Gzip
            ))
        ));
        assert!(matches!(
            parse_te("gzip;q=1.1"),
            Err(InvalidAcceptTransferEncodingValue::InvalidQValue)
        ));
        assert!(matches!(
            parse_te("gzip;foo=1"),
            Err(InvalidAcceptTransferEncodingValue::InvalidQValue)
        ));
        assert!(matches!(
            parse_te("gzip;q=0.5000"),
            Err(InvalidAcceptTransferEncodingValue::InvalidQValue)
        ));
    }

    #[test]
    fn te_incremental_and_display() {
        let mut v = AcceptTransferEncodingValue::default();
        v.parse(b"gzip;q=0.5").unwrap();
        v.parse(b"trailers").unwrap();
        assert!(v.trailers());
        assert_eq!(v.to_string(), "gzip;q=0.5, trailers");

        assert_eq!(
            parse_te("deflate, trailers").unwrap().to_string(),
            "deflate, trailers"
        );
        assert_eq!(TransferCodingQValue::ONE.to_string(), "1");
        assert_eq!(TransferCodingQValue(80).to_string(), "0.08");
        assert_eq!(TransferCodingQValue(8).to_string(), "0.008");
    }

    #[test]
    fn write_chunked_and_trailers() {
        let mut buf = Vec::new();
        TransferEncodingValue::default().write_chunked(b"Transfer-Encoding", &mut buf);
        assert!(buf.is_empty());

        TransferEncodingValue::CHUNKED.write_chunked(b"transfer-encoding", &mut buf);
        assert_eq!(buf, b"transfer-encoding: chunked\r\n");

        buf.clear();
        TransferEncodingValue::default().write(b"Transfer-Encoding", &mut buf);
        assert!(buf.is_empty());
        parse("gzip, chunked")
            .unwrap()
            .write(b"Transfer-Encoding", &mut buf);
        assert_eq!(buf, b"Transfer-Encoding: gzip, chunked\r\n");
        buf.clear();
        parse("gzip").unwrap().write(b"transfer-encoding", &mut buf);
        assert_eq!(buf, b"transfer-encoding: gzip\r\n");

        buf.clear();
        AcceptTransferEncodingValue::default().write_trailers(b"TE", &mut buf);
        assert!(buf.is_empty());

        let te = parse_te("trailers").unwrap();
        te.write_trailers(b"te", &mut buf);
        assert_eq!(buf, b"te: trailers\r\n");
    }
}
