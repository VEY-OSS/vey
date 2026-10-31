/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;

use super::{AnyServerConfig, ServerConfig, ServerConfigDiffAction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnixServerConfig {
    name: NodeName,
    pub(crate) listen: PathBuf,
}

impl UnixServerConfig {
    fn new() -> Self {
        UnixServerConfig {
            name: NodeName::default(),
            listen: PathBuf::new(),
        }
    }

    pub(super) fn parse(map: &yaml::Hash) -> anyhow::Result<Self> {
        let mut server = UnixServerConfig::new();
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
        if self.listen.as_os_str().is_empty() {
            return Err(anyhow!("listen path is not set"));
        }
        Ok(())
    }
}

impl ServerConfig for UnixServerConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction {
        let AnyServerConfig::Unix(new) = new else {
            return ServerConfigDiffAction::SpawnNew;
        };
        if self == new {
            ServerConfigDiffAction::NoAction
        } else {
            ServerConfigDiffAction::ReloadAndRespawn
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::ServerConfig;
    use super::*;

    fn parse(text: &str) -> anyhow::Result<UnixServerConfig> {
        let docs = yaml_rust::YamlLoader::load_from_str(text).unwrap();
        let Yaml::Hash(map) = &docs[0] else {
            panic!("yaml root should be a map");
        };
        UnixServerConfig::parse(map)
    }

    #[test]
    fn parse_unix_listen() {
        let config =
            parse("name: local_unix\ntype: unix\nlisten: /run/vey-dcgen/dcgen.sock\n").unwrap();
        assert_eq!(config.name().as_str(), "local_unix");
        assert_eq!(config.listen.as_os_str(), "/run/vey-dcgen/dcgen.sock");
    }

    #[test]
    fn parse_rejects_relative_path() {
        let err = parse("name: local_unix\ntype: unix\nlisten: dcgen.sock\n").unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.contains("absolute path"),
            "unexpected error: {message}"
        );
    }
}
