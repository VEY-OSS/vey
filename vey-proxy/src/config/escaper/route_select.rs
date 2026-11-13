/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::collection::{SelectivePickPolicy, WeightedValue};
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction};

const ESCAPER_CONFIG_TYPE: &str = "RouteSelect";

#[derive(Clone, PartialEq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = RouteSelect,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct RouteSelectEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) next_nodes: Vec<WeightedValue<NodeName>>,
    pub(crate) next_pick_policy: SelectivePickPolicy,
}

impl RouteSelectEscaperConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        RouteSelectEscaperConfig {
            name: NodeName::default(),
            position,
            next_nodes: Vec::new(),
            next_pick_policy: SelectivePickPolicy::Ketama,
        }
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut config = Self::new(position);

        vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;

        config.check()?;
        Ok(config)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_ESCAPER_TYPE => Ok(()),
            super::CONFIG_KEY_ESCAPER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "next_nodes" => {
                self.next_nodes =
                    vey_yaml::value::as_list(v, vey_yaml::value::as_weighted_metric_node_name)
                        .context(format!(
                            "invalid weighted metrics name list value for key {k}"
                        ))?;
                Ok(())
            }
            "next_pick_policy" => {
                self.next_pick_policy = vey_yaml::value::as_selective_pick_policy(v)
                    .context(format!("invalid selective pick policy value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.next_nodes.is_empty() {
            return Err(anyhow!("no next escapers found"));
        }
        self.next_nodes.reverse(); // reverse as we push to the back

        Ok(())
    }
}

impl EscaperConfig for RouteSelectEscaperConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &str {
        ESCAPER_CONFIG_TYPE
    }

    fn resolver(&self) -> &NodeName {
        Default::default()
    }

    fn dependent_escaper(&self) -> Option<BTreeSet<NodeName>> {
        let mut set = BTreeSet::new();
        for v in &self.next_nodes {
            set.insert(v.inner().clone());
        }
        Some(set)
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::RouteSelectEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfigDiffAction};

    fn parse(doc: &Yaml) -> RouteSelectEscaperConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        RouteSelectEscaperConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            next_nodes: next
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::RouteSelect(new)),
            EscaperConfigDiffAction::NoAction
        ));
    }
}
