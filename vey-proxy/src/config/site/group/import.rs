/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_types::metrics::NodeName;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteGroupImportConfig {
    site_group: NodeName,
    tags: BTreeSet<NodeName>,
}

impl SiteGroupImportConfig {
    pub(crate) fn site_group(&self) -> &NodeName {
        &self.site_group
    }

    pub(crate) fn tags(&self) -> &BTreeSet<NodeName> {
        &self.tags
    }

    pub(super) fn parse(v: &Yaml) -> anyhow::Result<Self> {
        let Yaml::Hash(map) = v else {
            return Err(anyhow!("site group import value should be a map"));
        };

        let mut config = SiteGroupImportConfig::default();
        vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;
        config.check()?;
        Ok(config)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "site_group" | "name" => {
                self.site_group = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "tag" | "tags" => {
                let list = vey_yaml::value::as_list(v, vey_yaml::value::as_metric_node_name)
                    .context(format!("invalid metric node name list for key {k}"))?;
                self.tags.extend(list);
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.site_group.is_empty() {
            return Err(anyhow!("site_group is not set"));
        }
        if self.tags.is_empty() {
            return Err(anyhow!("tags is not set"));
        }
        Ok(())
    }
}
