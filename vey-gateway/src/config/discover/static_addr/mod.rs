/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::net::SocketAddr;

use anyhow::anyhow;

use vey_macros::DiffAction;
use vey_types::collection::WeightedValue;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{
    AnyDiscoverConfig, CONFIG_KEY_DISCOVER_NAME, CONFIG_KEY_DISCOVER_TYPE, DiscoverConfig,
    DiscoverConfigDiffAction,
};

mod yaml;

const DISCOVER_CONFIG_TYPE: &str = "StaticAddr";

#[derive(Default, PartialEq, Eq)]
pub(crate) struct StaticAddrDiscoverInput {
    pub(crate) inner: Vec<WeightedValue<SocketAddr>>,
}

#[derive(Clone, DiffAction)]
#[diff_action(
    new = AnyDiscoverConfig,
    variant = StaticAddr,
    mismatch = DiscoverConfigDiffAction::SpawnNew,
    unchanged = DiscoverConfigDiffAction::NoAction,
)]
pub(crate) struct StaticAddrDiscoverConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl StaticAddrDiscoverConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        StaticAddrDiscoverConfig {
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

impl DiscoverConfig for StaticAddrDiscoverConfig {
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

    use super::StaticAddrDiscoverConfig;
    use crate::config::discover::{AnyDiscoverConfig, DiscoverConfigDiffAction};

    fn parse(doc: &Yaml) -> StaticAddrDiscoverConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        StaticAddrDiscoverConfig::parse_yaml_conf(map, None).unwrap()
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
            old.diff_action(&AnyDiscoverConfig::StaticAddr(new)),
            DiscoverConfigDiffAction::NoAction
        ));
    }
}
