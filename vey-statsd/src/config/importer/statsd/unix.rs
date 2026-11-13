/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyImporterConfig, ImporterConfig, ImporterConfigDiffAction};

const IMPORTER_CONFIG_TYPE: &str = "StatsD_UNIX";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyImporterConfig,
    variant = StatsDUnix,
    mismatch = ImporterConfigDiffAction::SpawnNew,
    unchanged = ImporterConfigDiffAction::NoAction,
    changed = ImporterConfigDiffAction::ReloadNoRespawn,
)]
pub(crate) struct StatsdUnixImporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) collector: NodeName,
    #[diff(changed = ImporterConfigDiffAction::ReloadAndRespawn)]
    pub(crate) listen: PathBuf,
}

impl StatsdUnixImporterConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        StatsdUnixImporterConfig {
            name: NodeName::default(),
            position,
            collector: Default::default(),
            listen: PathBuf::new(),
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut importer = StatsdUnixImporterConfig::new(position);

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
            "collector" => {
                self.collector = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "listen" => {
                self.listen = vey_yaml::value::as_absolute_path(v)
                    .context(format!("invalid unix listen path value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.collector.is_empty() {
            return Err(anyhow!("collector is not set"));
        }
        if self.listen.as_os_str().is_empty() {
            return Err(anyhow!("listen path is not set"));
        }

        Ok(())
    }
}

impl ImporterConfig for StatsdUnixImporterConfig {
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
        &self.collector
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::StatsdUnixImporterConfig;
    use crate::config::importer::{AnyImporterConfig, ImporterConfigDiffAction};

    fn parse(doc: &Yaml) -> StatsdUnixImporterConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        StatsdUnixImporterConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            collector: col
            listen: /tmp/diff-a.sock
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyImporterConfig::StatsDUnix(new)),
            ImporterConfigDiffAction::NoAction
        ));
    }
    #[test]
    fn changed_listen() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            collector: col
            listen: /tmp/diff-a.sock
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            collector: col
            listen: /tmp/diff-b.sock
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyImporterConfig::StatsDUnix(new)),
            ImporterConfigDiffAction::ReloadAndRespawn
        ));
    }
}
