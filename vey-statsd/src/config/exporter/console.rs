/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyExporterConfig, ExporterConfig, ExporterConfigDiffAction};

const EXPORTER_CONFIG_TYPE: &str = "Console";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyExporterConfig,
    variant = Console,
    mismatch = ExporterConfigDiffAction::SpawnNew,
    unchanged = ExporterConfigDiffAction::NoAction,
)]
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
        map: &saphyr::Mapping,
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
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::ConsoleExporterConfig;
    use crate::config::exporter::{AnyExporterConfig, ExporterConfigDiffAction};

    fn parse(doc: &Yaml) -> ConsoleExporterConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        ConsoleExporterConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyExporterConfig::Console(new)),
            ExporterConfigDiffAction::NoAction
        ));
    }
}
