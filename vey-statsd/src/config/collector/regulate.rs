/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::{MetricTagName, NodeName};
use vey_yaml::YamlDocPosition;

use super::{AnyCollectorConfig, CollectorConfig, CollectorConfigDiffAction};
use crate::types::MetricName;

const COLLECTOR_CONFIG_TYPE: &str = "Regulate";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyCollectorConfig,
    variant = Regulate,
    mismatch = CollectorConfigDiffAction::SpawnNew,
    unchanged = CollectorConfigDiffAction::NoAction,
    changed = CollectorConfigDiffAction::Reload,
)]
pub(crate) struct RegulateCollectorConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) prefix: Option<MetricName>,
    pub(crate) drop_tags: Vec<MetricTagName>,
    pub(crate) next: Option<NodeName>,
    pub(crate) exporters: Vec<NodeName>,
}

impl RegulateCollectorConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        RegulateCollectorConfig {
            name: NodeName::default(),
            position,
            prefix: None,
            drop_tags: Vec::new(),
            next: None,
            exporters: Vec::new(),
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = RegulateCollectorConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| collector.set(k, v))?;

        collector.check()?;
        Ok(collector)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_COLLECTOR_TYPE => Ok(()),
            super::CONFIG_KEY_COLLECTOR_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "prefix" => {
                let prefix = MetricName::parse_yaml(v)
                    .context(format!("invalid metric name value for key {k}"))?;
                self.prefix = Some(prefix);
                Ok(())
            }
            "drop_tags" => {
                self.drop_tags =
                    vey_yaml::value::as_list(v, vey_yaml::value::as_metric_tag_name)
                        .context(format!("invalid list of metric tag names for key {k}"))?;
                Ok(())
            }
            "next" => {
                let next = vey_yaml::value::as_metric_node_name(v)?;
                self.next = Some(next);
                Ok(())
            }
            "exporter" => {
                self.exporters = vey_yaml::value::as_list(v, vey_yaml::value::as_metric_node_name)
                    .context(format!("invalid list of exporter names for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        Ok(())
    }
}

impl CollectorConfig for RegulateCollectorConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn collector_type(&self) -> &'static str {
        COLLECTOR_CONFIG_TYPE
    }

    fn dependent_collector(&self) -> Option<BTreeSet<NodeName>> {
        let next = self.next.as_ref()?;
        let mut set = BTreeSet::new();
        set.insert(next.clone());
        Some(set)
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::RegulateCollectorConfig;
    use crate::config::collector::{AnyCollectorConfig, CollectorConfigDiffAction};

    fn parse(doc: &Yaml) -> RegulateCollectorConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        RegulateCollectorConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyCollectorConfig::Regulate(new)),
            CollectorConfigDiffAction::NoAction
        ));
    }
}
