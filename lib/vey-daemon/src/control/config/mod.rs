/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

mod local;

const DEFAULT_RECV_TIMEOUT: u64 = 30;
const DEFAULT_SEND_TIMEOUT: u64 = 1;

#[derive(Clone, Copy)]
pub(crate) struct GeneralControllerConfig {
    pub recv_timeout: u64,
    pub send_timeout: u64,
}

impl Default for GeneralControllerConfig {
    fn default() -> Self {
        GeneralControllerConfig::new()
    }
}

impl GeneralControllerConfig {
    pub(crate) const fn new() -> Self {
        GeneralControllerConfig {
            recv_timeout: DEFAULT_RECV_TIMEOUT,
            send_timeout: DEFAULT_SEND_TIMEOUT,
        }
    }

    pub(crate) fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "recv_timeout" => {
                let value =
                    vey_yaml::value::as_u64(v).context(format!("invalid u64 value for {k}"))?;
                self.recv_timeout = value;
                Ok(())
            }
            "send_timeout" => {
                let value =
                    vey_yaml::value::as_u64(v).context(format!("invalid u64 value for {k}"))?;
                self.send_timeout = value;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

pub(crate) use local::LocalControllerConfig;

pub fn load(v: &Yaml) -> anyhow::Result<()> {
    match v {
        Yaml::Hash(map) => {
            vey_yaml::foreach_kv(map, |k, v| match k {
                "local" => LocalControllerConfig::load(v),
                _ => Err(anyhow!("invalid key '{k}'")),
            })?;
            Ok(())
        }
        Yaml::Null => Ok(()),
        _ => Err(anyhow!("root value type should be hash")),
    }
}
