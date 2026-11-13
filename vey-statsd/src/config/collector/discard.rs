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

use super::{AnyCollectorConfig, CollectorConfig, CollectorConfigDiffAction};

const COLLECTOR_CONFIG_TYPE: &str = "Discard";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyCollectorConfig,
    variant = Discard,
    mismatch = CollectorConfigDiffAction::SpawnNew,
    unchanged = CollectorConfigDiffAction::NoAction,
)]
pub(crate) struct DiscardCollectorConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl DiscardCollectorConfig {
    pub(crate) fn with_name(name: &NodeName, position: Option<YamlDocPosition>) -> Self {
        DiscardCollectorConfig {
            name: name.clone(),
            position,
        }
    }

    fn new(position: Option<YamlDocPosition>) -> Self {
        DiscardCollectorConfig {
            name: NodeName::default(),
            position,
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = DiscardCollectorConfig::new(position);

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

impl CollectorConfig for DiscardCollectorConfig {
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
mod tests {
    use super::*;
    use saphyr::LoadableYamlNode;

    #[test]
    fn parse_ok_and_rejects_missing_name() {
        let docs = Yaml::load_from_str("name: drop\n").unwrap();
        let cfg = DiscardCollectorConfig::parse(docs[0].as_mapping().unwrap(), None).unwrap();
        assert_eq!(cfg.name().as_str(), "drop");
        assert_eq!(cfg.collector_type(), "Discard");

        let docs = Yaml::load_from_str("type: Discard\n").unwrap();
        assert!(DiscardCollectorConfig::parse(docs[0].as_mapping().unwrap(), None).is_err());
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::DiscardCollectorConfig;
    use crate::config::collector::{AnyCollectorConfig, CollectorConfigDiffAction};

    fn parse(doc: &Yaml) -> DiscardCollectorConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        DiscardCollectorConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyCollectorConfig::Discard(new)),
            CollectorConfigDiffAction::NoAction
        ));
    }
}
