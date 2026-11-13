/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{AnyBackendConfig, BackendConfig, BackendConfigDiffAction};

const BACKEND_CONFIG_TYPE: &str = "DummyClose";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyBackendConfig,
    variant = DummyClose,
    mismatch = BackendConfigDiffAction::SpawnNew,
    unchanged = BackendConfigDiffAction::NoAction,
)]
pub(crate) struct DummyCloseBackendConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
}

impl DummyCloseBackendConfig {
    pub(crate) fn new(name: &NodeName, position: Option<YamlDocPosition>) -> Self {
        DummyCloseBackendConfig {
            name: name.clone(),
            position,
        }
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut server = DummyCloseBackendConfig {
            name: NodeName::default(),
            position,
        };
        vey_yaml::foreach_kv(map, |k, v| server.set(k, v))?;
        server.check()?;
        Ok(server)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        Ok(())
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match k {
            super::CONFIG_KEY_BACKEND_TYPE => Ok(()),
            super::CONFIG_KEY_BACKEND_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

impl BackendConfig for DummyCloseBackendConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &'static str {
        BACKEND_CONFIG_TYPE
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::DummyCloseBackendConfig;
    use crate::config::backend::{AnyBackendConfig, BackendConfigDiffAction};

    fn parse(doc: &Yaml) -> DummyCloseBackendConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        DummyCloseBackendConfig::parse(map, None).unwrap()
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
            old.diff_action(&AnyBackendConfig::DummyClose(new)),
            BackendConfigDiffAction::NoAction
        ));
    }
}
