/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyCollectorConfig, CollectorConfig, CollectorConfigDiffAction};

const COLLECTOR_CONFIG_TYPE: &str = "Internal";

#[derive(Clone, Debug, Eq, PartialEq)]
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
        map: &yaml::Hash,
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

    fn diff_action(&self, new: &AnyCollectorConfig) -> CollectorConfigDiffAction {
        let AnyCollectorConfig::Internal(_new) = new else {
            return CollectorConfigDiffAction::SpawnNew;
        };

        CollectorConfigDiffAction::Update
    }
}
