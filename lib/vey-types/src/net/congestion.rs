/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::{fmt, io};

/// Maximum bytes accepted for a TCP congestion-control algorithm name.
///
/// Matches Linux `TCP_CA_NAME_MAX` (16), which counts the trailing NUL used by
/// the kernel option value; stored names are therefore at most 15 bytes.
const CONGESTION_ALGORITHM_MAX_LEN: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CongestionAlgorithm {
    buf: [u8; CONGESTION_ALGORITHM_MAX_LEN],
    len: usize,
}

impl CongestionAlgorithm {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        // ASCII was required in FromStr, so this is always valid UTF-8.
        std::str::from_utf8(self.as_bytes()).unwrap()
    }
}

impl AsRef<str> for CongestionAlgorithm {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for CongestionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for CongestionAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CongestionAlgorithm {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty congestion algorithm",
            ));
        }
        // Leave room for the kernel's trailing NUL (`TCP_CA_NAME_MAX`).
        if s.len() >= CONGESTION_ALGORITHM_MAX_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid congestion algorithm length",
            ));
        }
        if !s.is_ascii() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "congestion algorithm must be ASCII",
            ));
        }

        let mut buf = [0; CONGESTION_ALGORITHM_MAX_LEN];
        buf[..s.len()].copy_from_slice(s.as_bytes());

        Ok(CongestionAlgorithm { buf, len: s.len() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        let ca = CongestionAlgorithm::from_str("cubic").unwrap();
        assert!(!ca.is_empty());
        assert_eq!(ca.as_bytes(), b"cubic");
        assert_eq!(ca.as_str(), "cubic");
        assert_eq!(ca.to_string(), "cubic");
        assert_eq!(format!("{ca:?}"), "cubic");
    }

    #[test]
    fn rejects_empty_and_too_long() {
        assert!(CongestionAlgorithm::from_str("").is_err());
        assert!(CongestionAlgorithm::from_str("abcdefghijklmnop").is_err()); // 16
        assert!(CongestionAlgorithm::from_str("abcdefghijklmno").is_ok()); // 15
    }

    #[test]
    fn rejects_non_ascii() {
        assert!(CongestionAlgorithm::from_str("cübíc").is_err());
    }
}
