/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use yaml_rust::Yaml;

use vey_types::metrics::NodeName;

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub(crate) struct EgressIndexConfig {
    pub(crate) escaper: NodeName,
    pub(crate) number_index_key: String,
    pub(crate) string_index_key: String,
}

impl EgressIndexConfig {
    pub(super) fn parse(value: &Yaml) -> anyhow::Result<Self> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("egress upstream config should be a hash value"));
        };

        let mut conf = EgressIndexConfig::default();
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "escaper" => {
                conf.escaper = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "number_index_key" => {
                conf.number_index_key = vey_yaml::value::as_string(v)?;
                Ok(())
            }
            "string_index_key" => {
                conf.string_index_key = vey_yaml::value::as_string(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })?;

        conf.check()?;
        Ok(conf)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.escaper.is_empty() {
            return Err(anyhow!("no escaper name set"));
        }

        Ok(())
    }
}
