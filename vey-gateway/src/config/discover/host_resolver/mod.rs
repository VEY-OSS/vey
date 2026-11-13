/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use anyhow::anyhow;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_types::net::UpstreamAddr;
use vey_yaml::YamlDocPosition;

use super::{
    AnyDiscoverConfig, CONFIG_KEY_DISCOVER_NAME, CONFIG_KEY_DISCOVER_TYPE, DiscoverConfig,
    DiscoverConfigDiffAction,
};

mod yaml;

const DISCOVER_CONFIG_TYPE: &str = "HostResolver";

pub(crate) struct HostResolverDiscoverInput {
    pub(crate) addr: UpstreamAddr,
}

#[derive(Clone, PartialEq, Eq, DiffAction)]
#[diff_action(
    new = AnyDiscoverConfig,
    variant = HostResolver,
    mismatch = DiscoverConfigDiffAction::SpawnNew,
    unchanged = DiscoverConfigDiffAction::NoAction,
)]
pub(crate) struct HostResolverDiscoverConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl HostResolverDiscoverConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        HostResolverDiscoverConfig {
            name: NodeName::default(),
            position,
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        Ok(())
    }
}

impl DiscoverConfig for HostResolverDiscoverConfig {
    #[inline]
    fn name(&self) -> &NodeName {
        &self.name
    }

    #[inline]
    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    #[inline]
    fn r#type(&self) -> &'static str {
        DISCOVER_CONFIG_TYPE
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::HostResolverDiscoverConfig;
    use crate::config::discover::{AnyDiscoverConfig, DiscoverConfigDiffAction};

    fn parse(doc: &Yaml) -> HostResolverDiscoverConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        HostResolverDiscoverConfig::parse_yaml_conf(map, None).unwrap()
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
            old.diff_action(&AnyDiscoverConfig::HostResolver(new)),
            DiscoverConfigDiffAction::NoAction
        ));
    }
}
