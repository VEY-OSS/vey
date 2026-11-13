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

const EXPORTER_CONFIG_TYPE: &str = "Discard";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyExporterConfig,
    variant = Discard,
    mismatch = ExporterConfigDiffAction::SpawnNew,
    unchanged = ExporterConfigDiffAction::NoAction,
)]
pub(crate) struct DiscardExporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl DiscardExporterConfig {
    pub(crate) fn with_name(name: &NodeName, position: Option<YamlDocPosition>) -> Self {
        DiscardExporterConfig {
            name: name.clone(),
            position,
        }
    }

    fn new(position: Option<YamlDocPosition>) -> Self {
        DiscardExporterConfig {
            name: NodeName::default(),
            position,
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = DiscardExporterConfig::new(position);

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

impl ExporterConfig for DiscardExporterConfig {
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

    use super::DiscardExporterConfig;
    use crate::config::exporter::{AnyExporterConfig, ExporterConfigDiffAction};

    fn parse(doc: &Yaml) -> DiscardExporterConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        DiscardExporterConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyExporterConfig::Discard(new)),
            ExporterConfigDiffAction::NoAction
        ));
    }
}
