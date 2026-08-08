/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use jiff::Timestamp;
use serde_json::Value;

use vey_datetime::DateTimeParseExt;

pub fn as_rfc3339_datetime(v: &Value) -> anyhow::Result<Timestamp> {
    match v {
        Value::String(s) => {
            Timestamp::parse_rfc3339(s).map_err(|e| anyhow!("invalid rfc3339 datetime string: {e}"))
        }
        _ => Err(anyhow!(
            "json value type for 'rfc3339 datetime' should be string"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_rfc3339_datetime_ok() {
        let valid_cases = vec![
            "2023-01-01T00:00:00Z",
            "2025-12-31T23:59:59.999Z",
            "1990-06-15T12:30:45+08:00",
            "2000-02-29T14:30:00-05:00",
        ];

        for case in valid_cases {
            let value = Value::String(case.to_string());
            let result = as_rfc3339_datetime(&value).unwrap();
            let expected: Timestamp = case.parse().expect("valid RFC3339 string in test case");
            assert_eq!(result, expected);
        }
    }

    #[test]
    fn as_rfc3339_datetime_err() {
        let invalid_strings = vec![
            "",
            "invalid-date",
            "2023-13-01T00:00:00Z",
            "2023-02-30T00:00:00Z",
            "2023-01-01 00:00:00",
            "2023-01-01T25:00:00Z",
        ];

        for s in invalid_strings {
            let value = Value::String(s.to_string());
            assert!(as_rfc3339_datetime(&value).is_err());
        }

        let non_string_types = vec![
            Value::Null,
            Value::Bool(true),
            Value::Number(12345.into()),
            Value::Array(vec![]),
            Value::Object(serde_json::Map::new()),
        ];

        for value in non_string_types {
            assert!(as_rfc3339_datetime(&value).is_err());
        }
    }
}
