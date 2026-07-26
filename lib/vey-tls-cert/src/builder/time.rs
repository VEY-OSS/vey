/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use anyhow::anyhow;
use chrono::{DateTime, Datelike, Utc};
use openssl::asn1::Asn1Time;

pub(super) fn asn1_time_from_chrono(datetime: &DateTime<Utc>) -> anyhow::Result<Asn1Time> {
    let lazy_fmt = if datetime.year() >= 2050 {
        datetime.format_with_items(vey_datetime::format::asn1::RFC5280_GENERALIZED.iter())
    } else {
        datetime.format_with_items(vey_datetime::format::asn1::RFC5280_UTC.iter())
    };
    Asn1Time::from_str(&format!("{lazy_fmt}")).map_err(|e| anyhow!("failed to get asn1 time: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn utc_format_before_2050() {
        let dt = Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap();
        let asn1 = asn1_time_from_chrono(&dt).unwrap();
        assert_eq!(asn1.to_string(), "Jan 15 10:30:00 2024 GMT");
    }

    #[test]
    fn generalized_time_at_or_after_2050() {
        let dt = Utc.with_ymd_and_hms(2050, 6, 1, 0, 0, 0).unwrap();
        let asn1 = asn1_time_from_chrono(&dt).unwrap();
        assert_eq!(asn1.to_string(), "Jun  1 00:00:00 2050 GMT");
    }
}
