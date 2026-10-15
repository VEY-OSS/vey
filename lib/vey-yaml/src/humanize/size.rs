/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use humanize_rs::bytes::Bytes;
use yaml_rust::Yaml;

pub fn as_usize(v: &Yaml) -> anyhow::Result<usize> {
    match v {
        Yaml::String(value) => parse_bytes(value),
        Yaml::Integer(value) => Ok(usize::try_from(*value)?),
        _ => Err(anyhow!(
            "yaml value type for humanize usize should be 'string' or 'integer'"
        )),
    }
}

pub fn as_u64(v: &Yaml) -> anyhow::Result<u64> {
    match v {
        Yaml::String(value) => parse_bytes(value),
        Yaml::Integer(value) => Ok(u64::try_from(*value)?),
        _ => Err(anyhow!(
            "yaml value type for humanize u64 should be 'string' or 'integer'"
        )),
    }
}

pub fn as_u32(v: &Yaml) -> anyhow::Result<u32> {
    match v {
        Yaml::String(value) => parse_bytes(value),
        Yaml::Integer(value) => Ok(u32::try_from(*value)?),
        _ => Err(anyhow!(
            "yaml value type for humanize u32 should be 'string' or 'integer'"
        )),
    }
}

pub fn as_u16(v: &Yaml) -> anyhow::Result<u16> {
    match v {
        Yaml::String(value) => parse_bytes(value),
        Yaml::Integer(value) => Ok(u16::try_from(*value)?),
        _ => Err(anyhow!(
            "yaml value type for humanize u16 should be 'string' or 'integer'"
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

    #[test]
    fn as_usize_ok() {
        let v = yaml_str!("1000");
        assert_eq!(as_usize(&v).unwrap(), 1000);

        let v = yaml_str!("1K");
        assert_eq!(as_usize(&v).unwrap(), 1000);

        let v = yaml_str!("1KB");
        assert_eq!(as_usize(&v).unwrap(), 1000);

        let v = yaml_str!("1KiB");
        assert_eq!(as_usize(&v).unwrap(), 1024);

        let v = Yaml::Integer(1024);
        assert_eq!(as_usize(&v).unwrap(), 1024);
    }

    #[test]
    fn as_usize_err() {
        let v = Yaml::Integer(-1024);
        assert!(as_usize(&v).is_err());

        let v = Yaml::Real("1.01".to_string());
        assert!(as_usize(&v).is_err());

        let v = Yaml::Array(vec![Yaml::Integer(1)]);
        assert!(as_usize(&v).is_err());
    }

    #[test]
    fn as_u64_ok() {
        let v = yaml_str!("2000");
        assert_eq!(as_u64(&v).unwrap(), 2000);

        let v = Yaml::Integer(2048);
        assert_eq!(as_u64(&v).unwrap(), 2048);
    }

    #[test]
    fn as_u64_err() {
        let v = Yaml::Integer(-2048);
        assert!(as_u64(&v).is_err());

        let v = Yaml::Real("2.02".to_string());
        assert!(as_u64(&v).is_err());

        let v = Yaml::Boolean(true);
        assert!(as_u64(&v).is_err());
    }

    #[test]
    fn as_u32_ok() {
        let v = yaml_str!("4000");
        assert_eq!(as_u32(&v).unwrap(), 4000);

        let v = Yaml::Integer(4096);
        assert_eq!(as_u32(&v).unwrap(), 4096);
    }

    #[test]
    fn as_u32_err() {
        let v = Yaml::Integer(-4096);
        assert!(as_u32(&v).is_err());

        let v = Yaml::Real("4.04".to_string());
        assert!(as_u32(&v).is_err());

        let v = Yaml::Null;
        assert!(as_u32(&v).is_err());
    }

    #[test]
    fn as_u16_ok() {
        let v = yaml_str!("4000");
        assert_eq!(as_u16(&v).unwrap(), 4000);

        let v = Yaml::Integer(4096);
        assert_eq!(as_u16(&v).unwrap(), 4096);
    }

    #[test]
    fn as_u16_err() {
        let v = Yaml::Integer(-4096);
        assert!(as_u16(&v).is_err());

        let v = Yaml::Real("4.04".to_string());
        assert!(as_u16(&v).is_err());

        let v = Yaml::Null;
        assert!(as_u16(&v).is_err());
    }

    #[test]
    fn non_ascii_string_is_err() {
        // Non-ASCII is rejected before parse. `1·K`, `100°C`, and `１K` used to panic.
        for input in ["1·K", "100°C", "１K", "·1K", "1Ｋ", "1\u{00A0}KiB"] {
            let v = Yaml::String(input.to_string());
            assert!(as_usize(&v).is_err(), "{input}");
            assert!(as_u64(&v).is_err(), "{input}");
            assert!(as_u32(&v).is_err(), "{input}");
            assert!(as_u16(&v).is_err(), "{input}");
        }
    }
}
