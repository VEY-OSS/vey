/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use jiff::fmt::strtime;
use jiff::tz::TimeZone;
use jiff::{Error, Timestamp};

/// Parsing helpers for common datetime string shapes used across Vey.
pub trait DateTimeParseExt: Sized {
    /// Parse a Temporal/RFC3339-style instant (same as [`Timestamp`]'s [`FromStr`](std::str::FromStr)).
    ///
    /// Accepts a superset of strict RFC3339 (e.g. space instead of `T`, reduced precision).
    /// An offset (or `Z`) is required.
    fn parse_rfc3339(s: &str) -> Result<Self, Error>;

    /// Parse an FTP FACTS / RFC3659 timestamp (`YYYYMMDDHHMMSS[.fraction]`) as UTC.
    fn parse_rfc3659(s: &str) -> Result<Self, Error>;
}

impl DateTimeParseExt for Timestamp {
    #[inline]
    fn parse_rfc3339(s: &str) -> Result<Self, Error> {
        s.parse()
    }

    #[inline]
    fn parse_rfc3659(s: &str) -> Result<Self, Error> {
        let tm = strtime::parse("%Y%m%d%H%M%S%.f", s)?;
        let dt = tm.to_datetime()?;
        TimeZone::UTC.to_timestamp(dt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rfc3339_ok() {
        let ts = Timestamp::parse_rfc3339("2021-12-01T10:20:30.123Z").unwrap();
        assert_eq!(ts.to_string(), "2021-12-01T10:20:30.123Z");

        let ts = Timestamp::parse_rfc3339("2020-06-02T12:00:00+08:00").unwrap();
        assert_eq!(ts.to_string(), "2020-06-02T04:00:00Z");
    }

    #[test]
    fn parse_rfc3339_err() {
        assert!(Timestamp::parse_rfc3339("").is_err());
        assert!(Timestamp::parse_rfc3339("2021-12-01T10:20:30").is_err());
        assert!(Timestamp::parse_rfc3339("not-a-timestamp").is_err());
    }

    #[test]
    fn parse_rfc3659_ok() {
        let ts = Timestamp::parse_rfc3659("20211201102030").unwrap();
        assert_eq!(
            ts,
            Timestamp::parse_rfc3339("2021-12-01T10:20:30Z").unwrap()
        );

        let ts = Timestamp::parse_rfc3659("20211201102030.1").unwrap();
        assert_eq!(
            ts,
            Timestamp::parse_rfc3339("2021-12-01T10:20:30.1Z").unwrap()
        );

        let ts = Timestamp::parse_rfc3659("20211201102030.123").unwrap();
        assert_eq!(
            ts,
            Timestamp::parse_rfc3339("2021-12-01T10:20:30.123Z").unwrap()
        );
    }

    #[test]
    fn parse_rfc3659_err() {
        assert!(Timestamp::parse_rfc3659("").is_err());
        assert!(Timestamp::parse_rfc3659("2021").is_err());
        assert!(Timestamp::parse_rfc3659("not-a-timestamp").is_err());
        assert!(Timestamp::parse_rfc3659("20211301102030").is_err());
    }
}
