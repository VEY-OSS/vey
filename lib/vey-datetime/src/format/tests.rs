/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use chrono::format::{Parsed, parse};
use chrono::{DateTime, TimeZone, Utc};

use super::{asn1, ftp, log, std};

fn format_utc(fmt: &[chrono::format::Item<'_>], dt: &DateTime<Utc>) -> String {
    dt.format_with_items(fmt.iter()).to_string()
}

fn parse_utc(fmt: &[chrono::format::Item<'_>], s: &str) -> DateTime<Utc> {
    let mut parsed = Parsed::new();
    parse(&mut parsed, s, fmt.iter()).unwrap();
    parsed.to_datetime_with_timezone(&Utc).unwrap()
}

#[test]
fn rfc3339_fixed_microsecond_formats_and_roundtrips() {
    let dt = Utc.with_ymd_and_hms(2021, 12, 1, 10, 20, 30).unwrap();
    assert_eq!(
        format_utc(std::RFC3339_FIXED_MICROSECOND, &dt),
        "2021-12-01T10:20:30.000000Z"
    );

    let with_frac = DateTime::parse_from_rfc3339("2021-12-01T10:20:30.123456789Z")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(
        format_utc(std::RFC3339_FIXED_MICROSECOND, &with_frac),
        "2021-12-01T10:20:30.123456Z"
    );
}

#[test]
fn rfc5424_matches_syslog_timestamp_shape() {
    let dt = DateTime::parse_from_rfc3339("2021-12-01T10:20:30.123456789Z")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(
        format_utc(log::RFC5424, &dt),
        "2021-12-01T10:20:30.123456Z"
    );

    let dt = DateTime::parse_from_rfc3339("2021-12-01T10:20:30+08:00")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(format_utc(log::RFC5424, &dt), "2021-12-01T02:20:30.000000Z");
}

#[test]
fn rfc3164_and_stdio_log_formats() {
    let dt = Utc.with_ymd_and_hms(2021, 12, 1, 10, 20, 30).unwrap();
    assert_eq!(format_utc(log::RFC3164, &dt), "Dec  1 10:20:30");
    assert_eq!(format_utc(log::STDIO, &dt), "Dec 01 10:20:30.000");
}

#[test]
fn rfc3659_ftp_format_parses_compact_timestamp() {
    let dt = parse_utc(ftp::RFC3659, "20211201102030");
    let expected = DateTime::parse_from_rfc3339("2021-12-01T10:20:30+00:00")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(dt, expected);

    let dt = parse_utc(ftp::RFC3659, "20211201102030.123");
    let expected = DateTime::parse_from_rfc3339("2021-12-01T10:20:30.123+00:00")
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(dt, expected);
    assert_eq!(format_utc(ftp::RFC3659, &dt), "20211201102030.123");
}

#[test]
fn asn1_rfc5280_formats() {
    let dt = Utc.with_ymd_and_hms(2021, 12, 1, 10, 20, 30).unwrap();
    assert_eq!(format_utc(asn1::RFC5280_UTC, &dt), "211201102030Z");
    assert_eq!(format_utc(asn1::RFC5280_GENERALIZED, &dt), "20211201102030Z");

    let future = Utc.with_ymd_and_hms(2050, 1, 2, 3, 4, 5).unwrap();
    assert_eq!(format_utc(asn1::RFC5280_GENERALIZED, &future), "20500102030405Z");
}
