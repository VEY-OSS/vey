/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use super::BlendedRuntimeConfig;

impl BlendedRuntimeConfig {
    pub fn parse_by_yaml_kv(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "thread_number" => {
                let value = vey_yaml::value::as_usize(v)?;
                self.set_thread_number(value);
                Ok(())
            }
            "thread_name" => {
                let name = vey_yaml::value::as_ascii(v)
                    .context(format!("invalid ascii string value for key {k}"))?;
                self.set_thread_name(name.as_str());
                Ok(())
            }
            "thread_stack_size" => {
                let value = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                self.set_thread_stack_size(value);
                Ok(())
            }
            "max_io_events_per_tick" => {
                let capacity = vey_yaml::value::as_usize(v)?;
                self.set_max_io_events_per_tick(capacity);
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr::Scalar;
    use vey_yaml::yaml_str;

    #[test]
    fn parse_by_yaml_kv_ok() {
        let mut config = BlendedRuntimeConfig::new();

        let yaml = Yaml::Value(Scalar::Integer(4));
        assert!(config.parse_by_yaml_kv("thread_number", &yaml).is_ok());
        assert_eq!(config.thread_number, Some(4));

        let yaml = yaml_str!("worker_thread");
        assert!(config.parse_by_yaml_kv("thread_name", &yaml).is_ok());
        assert_eq!(config.thread_name, Some("worker_thread".to_string()));

        let yaml = Yaml::Value(Scalar::Integer(2048));
        assert!(config.parse_by_yaml_kv("thread_stack_size", &yaml).is_ok());
        assert_eq!(config.thread_stack_size, Some(2048));

        let yaml = yaml_str!("2K");
        assert!(config.parse_by_yaml_kv("thread_stack_size", &yaml).is_ok());
        assert_eq!(config.thread_stack_size, Some(2000));

        let yaml = Yaml::Value(Scalar::Integer(512));
        assert!(
            config
                .parse_by_yaml_kv("max_io_events_per_tick", &yaml)
                .is_ok()
        );
        assert_eq!(config.max_io_events_per_tick, Some(512));
    }

    #[test]
    fn parse_by_yaml_kv_err() {
        let mut config = BlendedRuntimeConfig::new();

        let yaml = Yaml::Value(Scalar::Integer(4));
        assert!(config.parse_by_yaml_kv("invalid_key", &yaml).is_err());

        let yaml = yaml_str!("four");
        assert!(config.parse_by_yaml_kv("thread_number", &yaml).is_err());

        let yaml = Yaml::Value(Scalar::Boolean(true));
        assert!(config.parse_by_yaml_kv("thread_name", &yaml).is_err());

        let yaml = yaml_str!("invalid_size");
        assert!(config.parse_by_yaml_kv("thread_stack_size", &yaml).is_err());

        let yaml = yaml_str!("five");
        assert!(
            config
                .parse_by_yaml_kv("max_io_events_per_tick", &yaml)
                .is_err()
        );
    }
}
