/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::OnceLock;

use anyhow::anyhow;
use yaml_rust::Yaml;

#[cfg(feature = "openssl-async-job")]
mod async_job;
#[cfg(feature = "openssl-async-job")]
pub(crate) use async_job::AsyncJobBackendConfig;

static BACKEND_CONFIG: OnceLock<BackendConfig> = OnceLock::new();

pub(crate) struct BackendConfig {
    pub(crate) dispatch_channel_size: usize,
    pub(crate) dispatch_counter_shift: u8,
    pub(crate) driver: BackendDriverConfig,
}

impl Default for BackendConfig {
    fn default() -> Self {
        BackendConfig::with_driver(BackendDriverConfig::Simple)
    }
}

impl BackendConfig {
    const fn with_driver(driver: BackendDriverConfig) -> Self {
        BackendConfig {
            dispatch_channel_size: 1024,
            dispatch_counter_shift: 3,
            driver,
        }
    }
}

pub(crate) enum BackendDriverConfig {
    Simple,
    #[cfg(feature = "openssl-async-job")]
    AsyncJob(AsyncJobBackendConfig),
}

pub(super) fn load(value: &Yaml) -> anyhow::Result<()> {
    let mut config = BackendConfig::default();
    match value {
        Yaml::Hash(map) => {
            vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                "dispatch_channel_size" => {
                    config.dispatch_channel_size = vey_yaml::value::as_usize(v)?;
                    Ok(())
                }
                "dispatch_counter_shift" => {
                    config.dispatch_counter_shift = vey_yaml::value::as_u8(v)?;
                    Ok(())
                }
                #[cfg(feature = "openssl-async-job")]
                "async_job" | "openssl_async_job" => {
                    let driver = AsyncJobBackendConfig::parse_yaml(v)?;
                    config.driver = BackendDriverConfig::AsyncJob(driver);
                    Ok(())
                }
                _ => Err(anyhow!("invalid key {k}")),
            })?;
        }
        Yaml::String(s) => match vey_yaml::key::normalize(s).as_str() {
            "simple" => {}
            #[cfg(feature = "openssl-async-job")]
            "async_job" | "openssl_async_job" => {
                config.driver = BackendDriverConfig::AsyncJob(AsyncJobBackendConfig::default());
            }
            _ => return Err(anyhow!("unsupported backend type {s}")),
        },
        _ => return Err(anyhow!("invalid yaml value type")),
    }
    BACKEND_CONFIG
        .set(config)
        .map_err(|_| anyhow!("backend config already set"))
}

pub(crate) fn get_config() -> &'static BackendConfig {
    BACKEND_CONFIG.get_or_init(BackendConfig::default)
}
