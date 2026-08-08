/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use openssl::asn1::Asn1Time;

use vey_datetime::DateTimeFormatExt;

pub(super) fn asn1_time_from_timestamp(datetime: &Timestamp) -> anyhow::Result<Asn1Time> {
    let zoned = datetime.to_zoned(TimeZone::UTC);
    let lazy_fmt = if zoned.year() >= 2050 {
        datetime.format_rfc5280_generalized()
    } else {
        datetime.format_rfc5280_utc()
    };
    Asn1Time::from_str(&format!("{lazy_fmt}")).map_err(|e| anyhow!("failed to get asn1 time: {e}"))
}

pub(super) fn timestamp_now_utc_zoned() -> jiff::Zoned {
    Timestamp::now().to_zoned(TimeZone::UTC)
}

pub(super) fn checked_sub_days(zoned: &jiff::Zoned, days: i64) -> anyhow::Result<Timestamp> {
    zoned
        .checked_sub(Span::new().days(days))
        .map(|z| z.timestamp())
        .map_err(|e| anyhow!("unable to get time before date: {e}"))
}

pub(super) fn checked_add_days(zoned: &jiff::Zoned, days: i64) -> anyhow::Result<Timestamp> {
    zoned
        .checked_add(Span::new().days(days))
        .map(|z| z.timestamp())
        .map_err(|e| anyhow!("unable to get time after date: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_format_before_2050() {
        let dt: Timestamp = "2024-01-15T10:30:00Z".parse().unwrap();
        let asn1 = asn1_time_from_timestamp(&dt).unwrap();
        assert_eq!(asn1.to_string(), "Jan 15 10:30:00 2024 GMT");
    }

    #[test]
    fn generalized_time_at_or_after_2050() {
        let dt: Timestamp = "2050-06-01T00:00:00Z".parse().unwrap();
        let asn1 = asn1_time_from_timestamp(&dt).unwrap();
        assert_eq!(asn1.to_string(), "Jun  1 00:00:00 2050 GMT");
    }
}
