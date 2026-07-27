/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use anyhow::anyhow;
use yaml_rust::Yaml;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimestampPrecision {
    Seconds,
    MilliSeconds,
    MicroSeconds,
    NanoSeconds,
}

impl TimestampPrecision {
    pub(crate) fn v2_query_value(self) -> &'static str {
        match self {
            Self::Seconds => "s",
            Self::MilliSeconds => "ms",
            Self::MicroSeconds => "us",
            Self::NanoSeconds => "ns",
        }
    }

    pub(crate) fn v3_query_value(self) -> &'static str {
        match self {
            Self::Seconds => "second",
            Self::MilliSeconds => "millisecond",
            Self::MicroSeconds => "microsecond",
            Self::NanoSeconds => "nanosecond",
        }
    }

    pub(crate) fn parse_yaml(value: &Yaml) -> anyhow::Result<Self> {
        if let Yaml::String(s) = value {
            TimestampPrecision::from_str(s)
        } else {
            Err(anyhow!(
                "yaml value type for timestamp precision should be string"
            ))
        }
    }
}

impl FromStr for TimestampPrecision {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "s" | "second" | "seconds" => Ok(TimestampPrecision::Seconds),
            "ms" | "millisecond" | "milliseconds" => Ok(TimestampPrecision::MilliSeconds),
            "us" | "microsecond" | "microseconds" => Ok(TimestampPrecision::MicroSeconds),
            "ns" | "nanosecond" | "nanoseconds" => Ok(TimestampPrecision::NanoSeconds),
            _ => Err(anyhow!("invalid timestamp precision: {s}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_aliases() {
        assert_eq!(
            TimestampPrecision::from_str("SECONDS").unwrap(),
            TimestampPrecision::Seconds
        );
        assert_eq!(
            TimestampPrecision::from_str("ms").unwrap(),
            TimestampPrecision::MilliSeconds
        );
        assert_eq!(
            TimestampPrecision::from_str("microseconds").unwrap(),
            TimestampPrecision::MicroSeconds
        );
        assert_eq!(
            TimestampPrecision::from_str("ns").unwrap(),
            TimestampPrecision::NanoSeconds
        );
        assert!(TimestampPrecision::from_str("hour").is_err());
    }

    #[test]
    fn query_values() {
        assert_eq!(TimestampPrecision::Seconds.v2_query_value(), "s");
        assert_eq!(TimestampPrecision::MilliSeconds.v2_query_value(), "ms");
        assert_eq!(
            TimestampPrecision::MicroSeconds.v3_query_value(),
            "microsecond"
        );
        assert_eq!(
            TimestampPrecision::NanoSeconds.v3_query_value(),
            "nanosecond"
        );
    }

    #[test]
    fn parse_yaml() {
        assert_eq!(
            TimestampPrecision::parse_yaml(&Yaml::String("us".into())).unwrap(),
            TimestampPrecision::MicroSeconds
        );
        assert!(TimestampPrecision::parse_yaml(&Yaml::Integer(1)).is_err());
    }
}
