/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use jiff::fmt::strtime;
use jiff::tz::Offset;
use jiff::{Timestamp, Zoned};

/// Formatting helpers for common datetime string shapes used across Vey.
pub trait DateTimeFormatExt {
    /// `YYYY-MM-DDTHH:MM:SS.ffffffZ` or with numeric offset when not UTC.
    fn format_rfc3339_fixed_microsecond(&self) -> strtime::Display<'_>;

    /// Same shape as RFC3339 fixed-microsecond (syslog RFC5424 TIMESTAMP).
    fn format_rfc5424(&self) -> strtime::Display<'_>;

    /// `Mon DD HH:MM:SS` with space-padded day (syslog RFC3164).
    fn format_rfc3164(&self) -> strtime::Display<'_>;

    /// `Mon DD HH:MM:SS.fff` with zero-padded day (stdio log).
    fn format_stdio(&self) -> strtime::Display<'_>;

    /// `YYYYMMDDHHMMSS[.fraction]` (FTP FACTS / RFC3659).
    fn format_rfc3659(&self) -> strtime::Display<'_>;

    /// `YYMMDDHHMMSSZ` (ASN.1 UTCTime).
    fn format_rfc5280_utc(&self) -> strtime::Display<'_>;

    /// `YYYYMMDDHHMMSSZ` (ASN.1 GeneralizedTime).
    fn format_rfc5280_generalized(&self) -> strtime::Display<'_>;
}

impl DateTimeFormatExt for Timestamp {
    #[inline]
    fn format_rfc3339_fixed_microsecond(&self) -> strtime::Display<'_> {
        self.strftime("%Y-%m-%dT%H:%M:%S%.6fZ")
    }

    #[inline]
    fn format_rfc5424(&self) -> strtime::Display<'_> {
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
    fn format_rfc3659(&self) -> strtime::Display<'_> {
        self.strftime("%Y%m%d%H%M%S%.f")
    }

    #[inline]
    fn format_rfc5280_utc(&self) -> strtime::Display<'_> {
        self.strftime("%y%m%d%H%M%SZ")
    }

    #[inline]
    fn format_rfc5280_generalized(&self) -> strtime::Display<'_> {
        self.strftime("%Y%m%d%H%M%SZ")
    }
}

impl DateTimeFormatExt for Zoned {
    #[inline]
    fn format_rfc3339_fixed_microsecond(&self) -> strtime::Display<'_> {
        if self.offset() == Offset::UTC {
            self.strftime("%Y-%m-%dT%H:%M:%S%.6fZ")
        } else {
            self.strftime("%Y-%m-%dT%H:%M:%S%.6f%:z")
        }
    }

    #[inline]
    fn format_rfc5424(&self) -> strtime::Display<'_> {
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
    fn format_rfc3659(&self) -> strtime::Display<'_> {
        self.strftime("%Y%m%d%H%M%S%.f")
    }

    #[inline]
    fn format_rfc5280_utc(&self) -> strtime::Display<'_> {
        self.strftime("%y%m%d%H%M%SZ")
    }

    #[inline]
    fn format_rfc5280_generalized(&self) -> strtime::Display<'_> {
        self.strftime("%Y%m%d%H%M%SZ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::tz::TimeZone;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
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
        assert_eq!(t.format_rfc5280_utc().to_string(), "211201102030Z");
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
    fn zoned_offset_uses_numeric_offset() {
        let z = ts("2021-12-01T02:20:30.123456Z").to_zoned(TimeZone::fixed(Offset::constant(8)));
        assert_eq!(
            z.format_rfc3339_fixed_microsecond().to_string(),
            "2021-12-01T10:20:30.123456+08:00"
        );
        assert_eq!(z.format_rfc3164().to_string(), "Dec  1 10:20:30");
    }
}
