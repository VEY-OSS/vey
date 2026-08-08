/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use jiff::Timestamp;
use yaml_rust::Yaml;

use vey_datetime::DateTimeParseExt;

pub fn as_rfc3339_datetime(value: &Yaml) -> anyhow::Result<Timestamp> {
    match value {
        Yaml::String(s) => {
            Timestamp::parse_rfc3339(s).map_err(|e| anyhow!("invalid rfc3339 datetime string: {e}"))
        }
        _ => Err(anyhow!(
            "yaml value type for 'rfc3339 datetime' should be string"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_rfc3339_datetime_ok() {
        let value = yaml_str!("2019-05-23T17:38:00Z");
        assert_eq!(
            as_rfc3339_datetime(&value).unwrap().to_string(),
            "2019-05-23T17:38:00Z"
        );

        let value = yaml_str!("2020-06-02T12:00:00+08:00");
        assert_eq!(
            as_rfc3339_datetime(&value).unwrap().to_string(),
            "2020-06-02T04:00:00Z"
        );

        let value = yaml_str!("2023-01-01T12:00:00-05:00");
        assert_eq!(
            as_rfc3339_datetime(&value).unwrap().to_string(),
            "2023-01-01T17:00:00Z"
        );

        let value = yaml_str!("2025-11-12T12:00:00.123Z");
        assert_eq!(
            as_rfc3339_datetime(&value).unwrap().to_string(),
            "2025-11-12T12:00:00.123Z"
        );

        let value = yaml_str!("2016-12-31T23:59:60Z");
        assert_eq!(
            as_rfc3339_datetime(&value).unwrap().to_string(),
            "2016-12-31T23:59:59Z"
        );
    }

    #[test]
    fn as_rfc3339_datetime_err() {
        let value = yaml_str!("2022-01-01T12:00:00");
        assert!(as_rfc3339_datetime(&value).is_err());

        let value = yaml_str!("2023-02-30T00:00:00Z");
        assert!(as_rfc3339_datetime(&value).is_err());

        let value = yaml_str!("2024-03-01T25:00:00Z");
        assert!(as_rfc3339_datetime(&value).is_err());

        let value = Yaml::Integer(12345);
        assert!(as_rfc3339_datetime(&value).is_err());

        let value = Yaml::Boolean(true);
        assert!(as_rfc3339_datetime(&value).is_err());
    }
}
