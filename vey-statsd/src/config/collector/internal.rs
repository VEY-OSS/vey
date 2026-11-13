/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyCollectorConfig, CollectorConfig, CollectorConfigDiffAction};

const COLLECTOR_CONFIG_TYPE: &str = "Internal";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyCollectorConfig,
    variant = Internal,
    mismatch = CollectorConfigDiffAction::SpawnNew,
    unchanged = CollectorConfigDiffAction::NoAction,
    changed = CollectorConfigDiffAction::Update,
)]
pub(crate) struct InternalCollectorConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) emit_interval: Duration,
    pub(crate) next: Option<NodeName>,
    pub(crate) exporters: Vec<NodeName>,
}

impl InternalCollectorConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        InternalCollectorConfig {
            name: NodeName::default(),
            position,
            emit_interval: Duration::from_secs(1),
            next: None,
            exporters: Vec::new(),
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = InternalCollectorConfig::new(position);

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
            "emit_interval" => {
                self.emit_interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
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

impl CollectorConfig for InternalCollectorConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn collector_type(&self) -> &'static str {
        COLLECTOR_CONFIG_TYPE
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::InternalCollectorConfig;
    use crate::config::collector::{AnyCollectorConfig, CollectorConfigDiffAction};

    fn parse(doc: &Yaml) -> InternalCollectorConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        InternalCollectorConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyCollectorConfig::Internal(new)),
            CollectorConfigDiffAction::NoAction
        ));
    }
}
