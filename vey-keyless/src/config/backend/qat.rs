/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::anyhow;
use yaml_rust::Yaml;

#[derive(Debug, Clone)]
pub(crate) struct QatBackendConfig {
    pub(crate) process_name: String,
    pub(crate) op_timeout: Duration,
}

impl Default for QatBackendConfig {
    fn default() -> Self {
        QatBackendConfig {
            process_name: "SSL".to_string(),
            op_timeout: Duration::from_secs(1),
        }
    }
}

impl QatBackendConfig {
    pub(super) fn parse_yaml(value: &Yaml) -> anyhow::Result<Self> {
        match value {
            Yaml::Hash(map) => {
                let mut config = QatBackendConfig::default();
                vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                    "process_name" => {
                        config.process_name = vey_yaml::value::as_string(v)?;
                        Ok(())
                    }
                    "op_timeout" => {
                        config.op_timeout = vey_yaml::humanize::as_duration(v)?;
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {k}")),
                })?;
                Ok(config)
            }
            Yaml::Null | Yaml::Boolean(true) => Ok(QatBackendConfig::default()),
            _ => Err(anyhow!("yaml value type for `qat` backend should be `map`")),
        }
    }
}
