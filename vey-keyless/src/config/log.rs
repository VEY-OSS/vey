/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_daemon::log::LogConfig;

static REQUEST_DEFAULT_LOG_CONFIG_CONTAINER: OnceLock<LogConfig> = OnceLock::new();
static TASK_DEFAULT_LOG_CONFIG_CONTAINER: OnceLock<LogConfig> = OnceLock::new();

pub(crate) fn load(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    let mut default_log_config: Option<LogConfig> = None;
    match v {
        Yaml::String(s) => {
            let config = LogConfig::with_driver_name(s, crate::build::PKG_NAME)?;
            default_log_config = Some(config);
        }
        Yaml::Hash(map) => {
            vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                "default" => {
                    let config = LogConfig::parse_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid value for key {k}"))?;
                    default_log_config = Some(config);
                    Ok(())
                }
                "syslog" => {
                    let config = LogConfig::parse_syslog_yaml(v, crate::build::PKG_NAME)
                        .context(format!("invalid syslog config value for key {k}"))?;
                    default_log_config = Some(config);
                    Ok(())
                }
                "fluentd" => {
                    let config = LogConfig::parse_fluentd_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid fluentd config value for key {k}"))?;
                    default_log_config = Some(config);
                    Ok(())
                }
                "request" => {
                    let config = LogConfig::parse_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid value for key {k}"))?;
                    REQUEST_DEFAULT_LOG_CONFIG_CONTAINER
                        .set(config)
                        .map_err(|_| anyhow!("request logger has already been set"))
                }
                "task" => {
                    let config = LogConfig::parse_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid value for key {k}"))?;
                    TASK_DEFAULT_LOG_CONFIG_CONTAINER
                        .set(config)
                        .map_err(|_| anyhow!("task logger has already been set"))
                }
                _ => Err(anyhow!("invalid key {k}")),
            })?;
        }
        Yaml::Null => return Ok(()),
        _ => return Err(anyhow!("invalid value type")),
    }
    if let Some(config) = default_log_config {
        let _ = REQUEST_DEFAULT_LOG_CONFIG_CONTAINER.set(config.clone());
        let _ = TASK_DEFAULT_LOG_CONFIG_CONTAINER.set(config);
    }
    Ok(())
}

pub(crate) fn get_request_default_config() -> LogConfig {
    REQUEST_DEFAULT_LOG_CONFIG_CONTAINER
        .get_or_init(|| LogConfig::new_discard(crate::build::PKG_NAME))
        .clone()
}

pub(crate) fn get_task_default_config() -> LogConfig {
    TASK_DEFAULT_LOG_CONFIG_CONTAINER
        .get_or_init(|| LogConfig::new_discard(crate::build::PKG_NAME))
        .clone()
}
