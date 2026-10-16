/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::fmt;

use jiff::fmt::strtime;
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, Zoned};

/// Formatting helpers for common datetime string shapes used across Vey.
pub trait DateTimeFormatExt {
    /// `YYYY-MM-DDTHH:MM:SS.ffffffZ` or with a numeric offset when not UTC.
    ///
    /// RFC 3339 numeric offsets are hours and minutes only. An offset that
    /// contains seconds is written as the same instant in UTC.
    fn format_rfc3339_fixed_microsecond(&self) -> impl fmt::Display + '_;

    /// Same shape as RFC3339 fixed-microsecond (syslog RFC5424 TIMESTAMP).
    fn format_rfc5424(&self) -> impl fmt::Display + '_;

    /// `Mon DD HH:MM:SS` with space-padded day (syslog RFC3164).
    fn format_rfc3164(&self) -> strtime::Display<'_>;

    /// `Mon DD HH:MM:SS.fff` with zero-padded day (stdio log).
    fn format_stdio(&self) -> strtime::Display<'_>;

    /// `YYYYMMDDHHMMSS[.fraction]` in UTC (FTP FACTS / RFC3659).
    fn format_rfc3659(&self) -> impl fmt::Display + '_;

    /// RFC 5280
    ///
    /// `YYMMDDHHMMSSZ` in UTC (RFC 5280 UTCTime) for 1950-2049
    /// `YYYYMMDDHHMMSSZ` in UTC (RFC 5280 GeneralizedTime) for 2050-
    fn format_rfc5280_auto(&self) -> impl fmt::Display + '_;

    /// `YYYYMMDDHHMMSSZ` in UTC (RFC 5280 GeneralizedTime).
    fn format_rfc5280_generalized(&self) -> impl fmt::Display + '_;
}

impl DateTimeFormatExt for Timestamp {
    #[inline]
    fn format_rfc3339_fixed_microsecond(&self) -> impl fmt::Display + '_ {
        self.strftime("%Y-%m-%dT%H:%M:%S%.6fZ")
    }

    #[inline]
    fn format_rfc5424(&self) -> impl fmt::Display + '_ {
        self.strftime("%Y-%m-%dT%H:%M:%S%.6fZ")
    }

    #[inline]
    fn format_rfc3164(&self) -> strtime::Display<'_> {
        self.strftime("%b %e %H:%M:%S")
    }

    #[inline]
    fn format_stdio(&self) -> strtime::Display<'_> {
        self.strftime("%b %d %H:%M:%S%.3f")
    }

    #[inline]
    fn format_rfc3659(&self) -> impl fmt::Display + '_ {
        self.strftime("%Y%m%d%H%M%S%.f")
    }

    fn format_rfc5280_auto(&self) -> impl fmt::Display + '_ {
        let year = self.to_zoned(TimeZone::UTC).year();
        if (1950..=2049).contains(&year) {
            self.strftime("%y%m%d%H%M%SZ")
        } else {
            self.strftime("%Y%m%d%H%M%SZ")
        }
    }

    #[inline]
    fn format_rfc5280_generalized(&self) -> impl fmt::Display + '_ {
        self.strftime("%Y%m%d%H%M%SZ")
    }
}

impl DateTimeFormatExt for Zoned {
    #[inline]
    fn format_rfc3339_fixed_microsecond(&self) -> impl fmt::Display + '_ {
        let offset = self.offset();
        if offset == Offset::UTC || offset_has_seconds(offset) {
            ZonedText::Utc(UtcDisplay {
                timestamp: self.timestamp(),
                pattern: "%Y-%m-%dT%H:%M:%S%.6fZ",
            })
        } else {
            ZonedText::Civil(self.strftime("%Y-%m-%dT%H:%M:%S%.6f%:z"))
        }
    }

    #[inline]
    fn format_rfc5424(&self) -> impl fmt::Display + '_ {
        self.format_rfc3339_fixed_microsecond()
    }

    #[inline]
    fn format_rfc3164(&self) -> strtime::Display<'_> {
        self.strftime("%b %e %H:%M:%S")
    }

    #[inline]
    fn format_stdio(&self) -> strtime::Display<'_> {
        self.strftime("%b %d %H:%M:%S%.3f")
    }

    #[inline]
    fn format_rfc3659(&self) -> impl fmt::Display + '_ {
        UtcDisplay {
            timestamp: self.timestamp(),
            pattern: "%Y%m%d%H%M%S%.f",
        }
    }

    fn format_rfc5280_auto(&self) -> impl fmt::Display + '_ {
        let year = self.timestamp().to_zoned(TimeZone::UTC).year();
        if (1950..=2049).contains(&year) {
            UtcDisplay {
                timestamp: self.timestamp(),
                pattern: "%y%m%d%H%M%SZ",
            }
        } else {
            UtcDisplay {
                timestamp: self.timestamp(),
                pattern: "%Y%m%d%H%M%SZ",
            }
        }
    }

    #[inline]
    fn format_rfc5280_generalized(&self) -> impl fmt::Display + '_ {
        UtcDisplay {
            timestamp: self.timestamp(),
            pattern: "%Y%m%d%H%M%SZ",
        }
    }
}

/// True when the offset is not an integral number of minutes.
fn offset_has_seconds(offset: Offset) -> bool {
    offset.seconds().rem_euclid(60) != 0
}

struct UtcDisplay {
    timestamp: Timestamp,
    pattern: &'static str,
}

impl fmt::Display for UtcDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.timestamp.strftime(self.pattern), f)
    }
}

enum ZonedText<'a> {
    Civil(strtime::Display<'a>),
    Utc(UtcDisplay),
}

impl fmt::Display for ZonedText<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Civil(display) => fmt::Display::fmt(display, f),
            Self::Utc(clock) => fmt::Display::fmt(clock, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn zoned(instant: &str, offset_seconds: i32) -> Zoned {
        ts(instant).to_zoned(TimeZone::fixed(
            Offset::from_seconds(offset_seconds).unwrap(),
        ))
    }

    #[test]
    fn rfc3339_fixed_microsecond() {
        let t = ts("2021-12-01T10:20:30Z");
        assert_eq!(
            t.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T10:20:30.000000Z"
        );

        let t = ts("2021-12-01T10:20:30.123456789Z");
        assert_eq!(
            t.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T10:20:30.123456Z"
        );
    }

    #[test]
    fn rfc5424() {
        let t = ts("2021-12-01T10:20:30.123456789Z");
        assert_eq!(
            t.format_rfc5424().to_string(),
            "2021-12-01T10:20:30.123456Z"
        );
    }

    #[test]
    fn rfc3164_and_stdio() {
        let t = ts("2021-12-01T10:20:30Z");
        assert_eq!(t.format_rfc3164().to_string(), "Dec  1 10:20:30");
        assert_eq!(t.format_stdio().to_string(), "Dec 01 10:20:30.000");
    }

    #[test]
    fn rfc3659() {
        let t = ts("2021-12-01T10:20:30Z");
        assert_eq!(t.format_rfc3659().to_string(), "20211201102030");

        let t = ts("2021-12-01T10:20:30.123Z");
        assert_eq!(t.format_rfc3659().to_string(), "20211201102030.123");
    }

    #[test]
    fn rfc5280() {
        let t = ts("2021-12-01T10:20:30Z");
        assert_eq!(t.format_rfc5280_auto().to_string(), "211201102030Z");
        assert_eq!(
            t.format_rfc5280_generalized().to_string(),
            "20211201102030Z"
        );

        let t = ts("2050-01-02T03:04:05Z");
        assert_eq!(
            t.format_rfc5280_generalized().to_string(),
            "20500102030405Z"
        );
    }

    #[test]
    fn rfc5280_utctime_year_window() {
        // RFC 5280 §4.1.2.5.1: YY >= 50 is 19YY, YY < 50 is 20YY.
        assert_eq!(
            ts("1950-01-01T00:00:00Z").format_rfc5280_auto().to_string(),
            "500101000000Z"
        );
        assert_eq!(
            ts("2049-12-31T23:59:59Z").format_rfc5280_auto().to_string(),
            "491231235959Z"
        );

        assert_eq!(
            ts("1949-06-15T00:00:00Z").format_rfc5280_auto().to_string(),
            "19490615000000Z"
        );
        assert_eq!(
            ts("0001-01-01T00:00:00Z").format_rfc5280_auto().to_string(),
            "00010101000000Z"
        );
    }

    #[test]
    fn zoned_offset_uses_numeric_offset() {
        let z = zoned("2021-12-01T02:20:30.123456Z", 8 * 60 * 60);
        assert_eq!(
            z.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T10:20:30.123456+08:00"
        );
        assert_eq!(z.format_rfc3164().to_string(), "Dec  1 10:20:30");

        let z = zoned("2021-12-01T10:20:30Z", 60);
        assert_eq!(
            z.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T10:21:30.000000+00:01"
        );
    }

    #[test]
    fn zoned_rfc3659_and_rfc5280_use_utc() {
        let z = zoned("2021-12-01T02:20:30.123Z", 8 * 60 * 60);
        assert_eq!(z.format_rfc3659().to_string(), "20211201022030.123");
        assert_eq!(z.format_rfc5280_auto().to_string(), "211201022030Z");
        assert_eq!(
            z.format_rfc5280_generalized().to_string(),
            "20211201022030Z"
        );

        let z = zoned("2021-12-01T10:20:30Z", -5 * 60 * 60);
        assert_eq!(z.format_rfc3659().to_string(), "20211201102030");
        assert_eq!(
            z.format_rfc5280_generalized().to_string(),
            "20211201102030Z"
        );

        // Local year is 2050, UTC year is still 2049.
        let z = zoned("2049-12-31T20:00:00Z", 8 * 60 * 60);
        assert_eq!(z.format_rfc5280_auto().to_string(), "491231200000Z");

        // Local year is 1950, UTC year is 1949.
        let z = zoned("1949-12-31T20:00:00Z", 8 * 60 * 60);
        assert_eq!(z.format_rfc5280_auto().to_string(), "19491231200000Z");
    }

    #[test]
    fn zoned_rfc3339_subminute_offset_uses_utc() {
        let z = zoned("2021-12-01T10:11:09.123456Z", 9 * 60 + 21);
        assert_eq!(
            z.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T10:11:09.123456Z"
        );
        assert_eq!(
            z.format_rfc5424().to_string(),
            "2021-12-01T10:11:09.123456Z"
        );

        let z = zoned("2021-12-01T00:00:01Z", -1);
        assert_eq!(
            z.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T00:00:01.000000Z"
        );
    }
}
