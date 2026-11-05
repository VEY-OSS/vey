/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::{Scalar, Yaml};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CryptoMbBackendConfig {}

impl CryptoMbBackendConfig {
    pub(super) fn parse_yaml(value: &Yaml) -> anyhow::Result<Self> {
        match value {
            Yaml::Mapping(map) => {
                vey_yaml::foreach_kv(map, |k, _v| Err(anyhow!("invalid key {k}")))?;
                Ok(CryptoMbBackendConfig::default())
            }
            Yaml::Value(Scalar::Null) | Yaml::Value(Scalar::Boolean(true)) => {
                Ok(CryptoMbBackendConfig::default())
            }
            _ => Err(anyhow!(
                "yaml value type for `crypto_mb` backend should be `map`"
            )),
        }
    }
}
