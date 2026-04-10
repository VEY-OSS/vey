/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS developers.
 */

use std::sync::OnceLock;

use anyhow::anyhow;
use yaml_rust::Yaml;

use super::GeneralControllerConfig;

#[derive(Default)]
pub(crate) struct LocalControllerConfig {
    general: GeneralControllerConfig,
}

static LOCAL_CONTROLLER_CONFIG: OnceLock<LocalControllerConfig> = OnceLock::new();

impl LocalControllerConfig {
    pub(crate) fn get_general() -> GeneralControllerConfig {
        LOCAL_CONTROLLER_CONFIG
            .get_or_init(|| LocalControllerConfig::default())
            .general
    }

    pub(crate) fn load(v: &Yaml) -> anyhow::Result<()> {
        match v {
            Yaml::Hash(map) => {
                let mut config = LocalControllerConfig::default();
                vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;
                LOCAL_CONTROLLER_CONFIG
                    .set(config)
                    .map_err(|_| anyhow!("local controller config has already been set"))
            }
            Yaml::Null => Ok(()),
            _ => Err(anyhow!("root value type should be hash")),
        }
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "recv_timeout" | "send_timeout" => self.general.set(k, v),
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}
