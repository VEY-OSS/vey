/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use humanize_rs::bytes::Bytes;
use serde_json::Value;

pub fn as_usize(v: &Value) -> anyhow::Result<usize> {
    match v {
        Value::String(s) => parse_bytes(s),
        Value::Number(n) => {
            if let Some(n) = n.as_u64() {
                Ok(usize::try_from(n)?)
            } else {
                Err(anyhow!("out of range json value for usize"))
            }
        }
        _ => Err(anyhow!(
            "yaml value type for humanize usize should be 'string' or 'integer'"
        )),
    }
}

pub fn as_u64(v: &Value) -> anyhow::Result<u64> {
    match v {
        Value::String(s) => parse_bytes(s),
        Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| anyhow!("out of range json value for u64")),
        _ => Err(anyhow!(
            "yaml value type for humanize u64 should be 'string' or 'integer'"
        )),
    }
}

/// Sizes are ASCII (`1KiB`). humanize-rs 0.1.5 splits on a char count and panics
/// when that index falls inside a multibyte character.
fn parse_bytes<T: humanize_rs::num::Int>(s: &str) -> anyhow::Result<T> {
    if !s.is_ascii() {
        return Err(anyhow!("invalid size value {s}"));
    }
    let size: Bytes<T> = s.parse()?;
    Ok(size.size())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn as_usize_ok() {
        let j = json!({"v": "1000"});
        assert_eq!(as_usize(&j["v"]).unwrap(), 1000);

        let j = json!({"v": "1K"});
        assert_eq!(as_usize(&j["v"]).unwrap(), 1000);

        let j = json!({"v": "1KB"});
        assert_eq!(as_usize(&j["v"]).unwrap(), 1000);

        let j = json!({"v": "1KiB"});
        assert_eq!(as_usize(&j["v"]).unwrap(), 1024);

        let j = json!({"v": 1024});
        assert_eq!(as_usize(&j["v"]).unwrap(), 1024);
    }

    #[test]
    fn as_usize_err() {
        let j = json!({"v": -1024});
        assert!(as_usize(&j["v"]).is_err());

        let j = json!({"v": 1.01});
        assert!(as_usize(&j["v"]).is_err());

        let j = json!({"v": ["1"]});
        assert!(as_usize(&j["v"]).is_err());
    }

    #[test]
    fn as_u64_ok() {
        let j = json!({"v": "1000"});
        assert_eq!(as_u64(&j["v"]).unwrap(), 1000);

        let j = json!({"v": "1K"});
        assert_eq!(as_u64(&j["v"]).unwrap(), 1000);

        let j = json!({"v": "1KB"});
        assert_eq!(as_u64(&j["v"]).unwrap(), 1000);

        let j = json!({"v": "1KiB"});
        assert_eq!(as_u64(&j["v"]).unwrap(), 1024);

        let j = json!({"v": 1024});
        assert_eq!(as_u64(&j["v"]).unwrap(), 1024);
    }

    #[test]
    fn as_u64_err() {
        let j = json!({"v": -1024});
        assert!(as_u64(&j["v"]).is_err());

        let j = json!({"v": 1.01});
        assert!(as_u64(&j["v"]).is_err());

        let j = json!({"v": ["1"]});
        assert!(as_u64(&j["v"]).is_err());
    }

    #[test]
    fn non_ascii_string_is_err() {
        // Non-ASCII is rejected before parse. `1·K`, `100°C`, and `１K` used to panic.
        for input in ["1·K", "100°C", "１K", "·1K", "1Ｋ", "1\u{00A0}KiB"] {
            let j = json!({ "v": input });
            assert!(as_usize(&j["v"]).is_err(), "{input}");
            assert!(as_u64(&j["v"]).is_err(), "{input}");
        }
    }
}
