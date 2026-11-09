/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::Yaml;

use vey_yaml::YamlDocPosition;

use super::{HostResolverDiscoverConfig, HostResolverDiscoverInput};

impl HostResolverDiscoverConfig {
    pub(crate) fn parse_yaml_conf(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut site = HostResolverDiscoverConfig::new(position);
        vey_yaml::foreach_kv(map, |k, v| site.set_yaml(k, v))?;
        site.check()?;
        Ok(site)
    }

    fn set_yaml(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match k {
            super::CONFIG_KEY_DISCOVER_TYPE => Ok(()),
            super::CONFIG_KEY_DISCOVER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    pub(crate) fn parse_yaml_data(
        &self,
        input: &Yaml,
    ) -> anyhow::Result<HostResolverDiscoverInput> {
        let addr = vey_yaml::value::as_upstream_addr(input, 0)?;
        Ok(HostResolverDiscoverInput { addr })
    }
}
