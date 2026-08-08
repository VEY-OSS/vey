/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use jiff::{Error, Timestamp};

use vey_datetime::DateTimeParseExt;

#[inline]
pub(crate) fn parse_from_str(s: &str) -> Result<Timestamp, Error> {
    Timestamp::parse_rfc3659(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_no_dot() {
        let dt = parse_from_str("20211201102030").unwrap();
        let expected = Timestamp::parse_rfc3339("2021-12-01T10:20:30Z").unwrap();
        assert_eq!(dt, expected);
    }

    #[test]
    fn parse_dot_1() {
        let dt = parse_from_str("20211201102030.1").unwrap();
        let expected = Timestamp::parse_rfc3339("2021-12-01T10:20:30.1Z").unwrap();
        assert_eq!(dt, expected);
    }

    #[test]
    fn parse_dot_3() {
        let dt = parse_from_str("20211201102030.123").unwrap();
        let expected = Timestamp::parse_rfc3339("2021-12-01T10:20:30.123Z").unwrap();
        assert_eq!(dt, expected);
    }

    #[test]
    fn parse_rejects_invalid() {
        assert!(parse_from_str("").is_err());
        assert!(parse_from_str("2021").is_err());
        assert!(parse_from_str("not-a-timestamp").is_err());
        assert!(parse_from_str("20211301102030").is_err());
    }
}
