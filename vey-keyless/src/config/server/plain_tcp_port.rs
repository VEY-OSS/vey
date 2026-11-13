/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_types::net::{ProxyProtocolVersion, TcpListenConfig};
use vey_yaml::YamlDocPosition;

use super::{AnyKeyServerConfig, KeyServerConfig, KeyServerConfigDiffAction};

pub(crate) const SERVER_CONFIG_TYPE: &str = "PlainTcpPort";

#[derive(Clone, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyKeyServerConfig,
    variant = PlainTcpPort,
    mismatch = KeyServerConfigDiffAction::SpawnNew,
    unchanged = KeyServerConfigDiffAction::NoAction,
    changed = KeyServerConfigDiffAction::ReloadAndRespawn,
)]
pub(crate) struct PlainTcpPortConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) listen: TcpListenConfig,
    pub(crate) server: NodeName,
    pub(crate) proxy_protocol: Option<ProxyProtocolVersion>,
    pub(crate) proxy_protocol_read_timeout: Duration,
}

impl PlainTcpPortConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        PlainTcpPortConfig {
            name: NodeName::default(),
            position,
            listen: TcpListenConfig::default(),
            server: NodeName::default(),
            proxy_protocol: None,
            proxy_protocol_read_timeout: Duration::from_secs(5),
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut server = PlainTcpPortConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| server.set(k, v))?;

        server.check()?;
        Ok(server)
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.server.is_empty() {
            return Err(anyhow!("server is not set"));
        }
        self.listen.check().context("invalid listen config")?;
        Ok(())
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_SERVER_TYPE => Ok(()),
            super::CONFIG_KEY_SERVER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "listen" => {
                self.listen = vey_yaml::value::as_tcp_listen_config(v)
                    .context(format!("invalid tcp listen config value for key {k}"))?;
                Ok(())
            }
            "server" => {
                self.server = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "proxy_protocol" => {
                let p = vey_yaml::value::as_proxy_protocol_version(v)
                    .context(format!("invalid proxy protocol version value for key {k}"))?;
                self.proxy_protocol = Some(p);
                Ok(())
            }
            "proxy_protocol_read_timeout" => {
                let t = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                self.proxy_protocol_read_timeout = t;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

impl KeyServerConfig for PlainTcpPortConfig {
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
        SERVER_CONFIG_TYPE
    }

    fn dependent_server(&self) -> Option<NodeName> {
        Some(self.server.clone())
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::PlainTcpPortConfig;
    use crate::config::server::{AnyKeyServerConfig, KeyServerConfigDiffAction};

    fn parse(doc: &Yaml) -> PlainTcpPortConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        PlainTcpPortConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen: 127.0.0.1:18080
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyKeyServerConfig::PlainTcpPort(new)),
            KeyServerConfigDiffAction::NoAction
        ));
    }
}
