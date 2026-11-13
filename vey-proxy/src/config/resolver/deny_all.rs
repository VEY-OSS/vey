/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::anyhow;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyResolverConfig, ResolverConfig, ResolverConfigDiffAction};

const RESOLVER_CONFIG_TYPE: &str = "deny-all";

#[derive(Clone, DiffAction)]
#[diff_action(
    new = AnyResolverConfig,
    variant = DenyAll,
    mismatch = ResolverConfigDiffAction::SpawnNew,
    unchanged = ResolverConfigDiffAction::NoAction,
)]
pub(crate) struct DenyAllResolverConfig {
    position: Option<YamlDocPosition>,
    name: NodeName,
}

impl DenyAllResolverConfig {
    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut resolver = DenyAllResolverConfig {
            position,
            name: NodeName::default(),
        };

        vey_yaml::foreach_kv(map, |k, v| resolver.set(k, v))?;

        resolver.check()?;
        Ok(resolver)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }

        Ok(())
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_RESOLVER_TYPE => Ok(()),
            super::CONFIG_KEY_RESOLVER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

impl ResolverConfig for DenyAllResolverConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &'static str {
        RESOLVER_CONFIG_TYPE
    }

    fn dependent_resolver(&self) -> Option<BTreeSet<NodeName>> {
        None
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::DenyAllResolverConfig;
    use crate::config::resolver::{AnyResolverConfig, ResolverConfigDiffAction};

    fn parse(doc: &Yaml) -> DenyAllResolverConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        DenyAllResolverConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyResolverConfig::DenyAll(new)),
            ResolverConfigDiffAction::NoAction
        ));
    }
}
