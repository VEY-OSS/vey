/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_daemon::log::LogConfig;

static RESOLVE_DEFAULT_LOG_CONFIG_CONTAINER: OnceLock<LogConfig> = OnceLock::new();
static ESCAPE_DEFAULT_LOG_CONFIG_CONTAINER: OnceLock<LogConfig> = OnceLock::new();
static AUDIT_DEFAULT_LOG_CONFIG_CONTAINER: OnceLock<LogConfig> = OnceLock::new();
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
                "resolve" => {
                    let config = LogConfig::parse_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid value for key {k}"))?;
                    RESOLVE_DEFAULT_LOG_CONFIG_CONTAINER
                        .set(config)
                        .map_err(|_| anyhow!("resolver logger has already been set"))
                }
                "escape" => {
                    let config = LogConfig::parse_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid value for key {k}"))?;
                    ESCAPE_DEFAULT_LOG_CONFIG_CONTAINER
                        .set(config)
                        .map_err(|_| anyhow!("escape logger has already been set"))
                }
                "audit" => {
                    let config = LogConfig::parse_yaml(v, conf_dir, crate::build::PKG_NAME)
                        .context(format!("invalid value for key {k}"))?;
                    AUDIT_DEFAULT_LOG_CONFIG_CONTAINER
                        .set(config)
                        .map_err(|_| anyhow!("audit logger has already been set"))
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
        let _ = RESOLVE_DEFAULT_LOG_CONFIG_CONTAINER.set(config.clone());
        let _ = ESCAPE_DEFAULT_LOG_CONFIG_CONTAINER.set(config.clone());
        let _ = AUDIT_DEFAULT_LOG_CONFIG_CONTAINER.set(config.clone());
        let _ = TASK_DEFAULT_LOG_CONFIG_CONTAINER.set(config);
    }
    Ok(())
}

pub(crate) fn get_resolve_default_config() -> LogConfig {
    RESOLVE_DEFAULT_LOG_CONFIG_CONTAINER
        .get_or_init(|| LogConfig::new_discard(crate::build::PKG_NAME))
        .clone()
}

pub(crate) fn get_escape_default_config() -> LogConfig {
    ESCAPE_DEFAULT_LOG_CONFIG_CONTAINER
        .get_or_init(|| LogConfig::new_discard(crate::build::PKG_NAME))
        .clone()
}

pub(crate) fn get_audit_default_config() -> LogConfig {
    AUDIT_DEFAULT_LOG_CONFIG_CONTAINER
        .get_or_init(|| LogConfig::new_discard(crate::build::PKG_NAME))
        .clone()
}

pub(crate) fn get_task_default_config() -> LogConfig {
    TASK_DEFAULT_LOG_CONFIG_CONTAINER
        .get_or_init(|| LogConfig::new_discard(crate::build::PKG_NAME))
        .clone()
}
