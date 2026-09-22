/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_types::metrics::NodeName;

#[derive(Clone, Debug, PartialEq, Eq)]
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

        let mut site_group = NodeName::default();
        let mut tags = BTreeSet::new();
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "site_group" | "name" => {
                site_group = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "tag" | "tags" => {
                let list = vey_yaml::value::as_list(v, vey_yaml::value::as_metric_node_name)
                    .context(format!("invalid metric node name list for key {k}"))?;
                tags = list.into_iter().collect();
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })?;

        if site_group.is_empty() {
            return Err(anyhow!("site_group is not set"));
        }
        if tags.is_empty() {
            return Err(anyhow!("no tags set"));
        }
        Ok(SiteGroupImportConfig { site_group, tags })
    }
}
