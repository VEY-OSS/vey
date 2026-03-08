/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2025 ByteDance and/or its affiliates.
 */

use anyhow::anyhow;
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyExporterConfig, ExporterConfig, ExporterConfigDiffAction};

const EXPORTER_CONFIG_TYPE: &str = "Console";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConsoleExporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl ConsoleExporterConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        ConsoleExporterConfig {
            name: NodeName::default(),
            position,
        }
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = ConsoleExporterConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| collector.set(k, v))?;

        collector.check()?;
        Ok(collector)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_EXPORTER_TYPE => Ok(()),
            super::CONFIG_KEY_EXPORTER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
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

impl ExporterConfig for ConsoleExporterConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn exporter_type(&self) -> &'static str {
        EXPORTER_CONFIG_TYPE
    }

    fn diff_action(&self, new: &AnyExporterConfig) -> ExporterConfigDiffAction {
        let AnyExporterConfig::Console(_new) = new else {
            return ExporterConfigDiffAction::SpawnNew;
        };

        ExporterConfigDiffAction::NoAction
    }
}
