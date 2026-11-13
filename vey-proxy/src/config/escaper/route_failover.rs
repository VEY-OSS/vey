/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::anyhow;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction};

const ESCAPER_CONFIG_TYPE: &str = "RouteFailover";

#[derive(Clone, PartialEq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = RouteFailover,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct RouteFailoverEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) primary_node: NodeName,
    pub(crate) standby_node: NodeName,
    pub(crate) fallback_delay: Duration,
}

impl RouteFailoverEscaperConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        RouteFailoverEscaperConfig {
            name: NodeName::default(),
            position,
            primary_node: NodeName::default(),
            standby_node: NodeName::default(),
            fallback_delay: Duration::from_millis(100),
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
            "primary" | "primary_next" => {
                self.primary_node = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "standby" | "standby_next" => {
                self.standby_node = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "fallback_delay" | "delay" | "fallback_timeout" | "timeout" => {
                self.fallback_delay = vey_yaml::humanize::as_duration(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.primary_node.is_empty() {
            return Err(anyhow!("no primary next escaper set"));
        }
        if self.standby_node.is_empty() {
            return Err(anyhow!("no standby next escaper set"));
        }

        Ok(())
    }
}

impl EscaperConfig for RouteFailoverEscaperConfig {
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
        set.insert(self.primary_node.clone());
        set.insert(self.standby_node.clone());
        Some(set)
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::RouteFailoverEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfigDiffAction};

    fn parse(doc: &Yaml) -> RouteFailoverEscaperConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        RouteFailoverEscaperConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            primary: pri
            standby: sec
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::RouteFailover(new)),
            EscaperConfigDiffAction::NoAction
        ));
    }
}
