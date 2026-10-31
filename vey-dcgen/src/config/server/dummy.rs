/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;

use super::{AnyServerConfig, ServerConfig, ServerConfigDiffAction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DummyServerConfig {
    name: NodeName,
}

impl DummyServerConfig {
    pub(crate) fn new(name: &NodeName) -> Self {
        DummyServerConfig { name: name.clone() }
    }

    pub(super) fn parse(map: &yaml::Hash) -> anyhow::Result<Self> {
        let mut server = DummyServerConfig {
            name: NodeName::default(),
        };
        vey_yaml::foreach_kv(map, |k, v| server.set(k, v))?;
        server.check()?;
        Ok(server)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_SERVER_TYPE => Ok(()),
            "name" => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        Ok(())
    }
}

impl ServerConfig for DummyServerConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction {
        let AnyServerConfig::Dummy(new) = new else {
            return ServerConfigDiffAction::SpawnNew;
        };
        if self == new {
            ServerConfigDiffAction::NoAction
        } else {
            ServerConfigDiffAction::ReloadAndRespawn
        }
    }
}
