/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use jiff::Timestamp;
use rmpv::ValueRef;

use vey_datetime::DateTimeParseExt;

pub fn as_rfc3339_datetime(value: &ValueRef) -> anyhow::Result<Timestamp> {
    match value {
        ValueRef::String(s) => match s.as_str() {
            Some(s) => Timestamp::parse_rfc3339(s)
                .map_err(|e| anyhow!("invalid rfc3339 datetime string: {e}")),
            None => Err(anyhow!("invalid utf-8 string")),
        },
        _ => Err(anyhow!(
            "msgpack value type for 'rfc3339 datetime' should be string"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmpv::Utf8StringRef;

    #[test]
    fn as_rfc3339_datetime_ok() {
        let value = ValueRef::String(Utf8StringRef::from("2019-05-23T17:38:00Z"));
        let dt = as_rfc3339_datetime(&value).unwrap();
        assert_eq!(dt.to_string(), "2019-05-23T17:38:00Z");
    }

    #[test]
    fn as_rfc3339_datetime_err() {
        let value = ValueRef::String(Utf8StringRef::from("2019-05-23 17:38:00"));
        assert!(as_rfc3339_datetime(&value).is_err());

        let value = ValueRef::F32(1.0);
        assert!(as_rfc3339_datetime(&value).is_err());
    }
}
