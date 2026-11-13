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

use super::{AnyImporterConfig, ImporterConfig, ImporterConfigDiffAction};

const IMPORTER_CONFIG_TYPE: &str = "Dummy";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyImporterConfig,
    variant = Dummy,
    mismatch = ImporterConfigDiffAction::SpawnNew,
    unchanged = ImporterConfigDiffAction::NoAction,
)]
pub(crate) struct DummyImporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl DummyImporterConfig {
    pub(crate) fn with_name(name: &NodeName, position: Option<YamlDocPosition>) -> Self {
        DummyImporterConfig {
            name: name.clone(),
            position,
        }
    }

    fn new(position: Option<YamlDocPosition>) -> Self {
        DummyImporterConfig {
            name: NodeName::default(),
            position,
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut importer = DummyImporterConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| importer.set(k, v))?;

        importer.check()?;
        Ok(importer)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_IMPORTER_TYPE => Ok(()),
            super::CONFIG_KEY_IMPORTER_NAME => {
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

impl ImporterConfig for DummyImporterConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn importer_type(&self) -> &'static str {
        IMPORTER_CONFIG_TYPE
    }

    fn collector(&self) -> &NodeName {
        Default::default()
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::DummyImporterConfig;
    use crate::config::importer::{AnyImporterConfig, ImporterConfigDiffAction};

    fn parse(doc: &Yaml) -> DummyImporterConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        DummyImporterConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyImporterConfig::Dummy(new)),
            ImporterConfigDiffAction::NoAction
        ));
    }
}
