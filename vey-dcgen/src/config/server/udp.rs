/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;
use vey_types::net::UdpListenConfig;

use super::{AnyServerConfig, ServerConfig, ServerConfigDiffAction};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UdpServerConfig {
    name: NodeName,
    pub(crate) listen: UdpListenConfig,
}

impl UdpServerConfig {
    fn new() -> Self {
        UdpServerConfig {
            name: NodeName::default(),
            listen: UdpListenConfig::default(),
        }
    }

    pub(super) fn parse(map: &yaml::Hash) -> anyhow::Result<Self> {
        let mut server = UdpServerConfig::new();
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
                self.listen = vey_yaml::value::as_udp_listen_config(v)
                    .context(format!("invalid udp listen config value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        self.listen.check().context("invalid listen config")?;
        Ok(())
    }
}

impl ServerConfig for UdpServerConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction {
        let AnyServerConfig::Udp(new) = new else {
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

    fn parse(text: &str) -> anyhow::Result<UdpServerConfig> {
        let docs = yaml_rust::YamlLoader::load_from_str(text).unwrap();
        let Yaml::Hash(map) = &docs[0] else {
            panic!("yaml root should be a map");
        };
        UdpServerConfig::parse(map)
    }

    #[test]
    fn parse_udp_listen() {
        let config = parse("name: local\ntype: udp\nlisten: 127.0.0.1:2999\n").unwrap();
        assert_eq!(config.name().as_str(), "local");
        assert_eq!(config.listen.address().to_string(), "127.0.0.1:2999");
    }

    #[test]
    fn parse_requires_name_and_listen() {
        let err = parse("type: udp\nlisten: 127.0.0.1:2999\n").unwrap_err();
        assert!(err.to_string().contains("name is not set"));

        let err = parse("name: local\ntype: udp\n").unwrap_err();
        assert!(err.to_string().contains("invalid listen config"));
    }
}
