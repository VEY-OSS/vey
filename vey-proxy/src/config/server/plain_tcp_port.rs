/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::acl::AclNetworkRuleBuilder;
use vey_types::metrics::NodeName;
use vey_types::net::{ProxyProtocolVersion, TcpListenConfig};
use vey_yaml::YamlDocPosition;

use super::ServerConfig;
use crate::config::server::{AnyServerConfig, ServerConfigDiffAction};

const SERVER_CONFIG_TYPE: &str = "PlainTcpPort";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyServerConfig,
    variant = PlainTcpPort,
    mismatch = ServerConfigDiffAction::SpawnNew,
    unchanged = ServerConfigDiffAction::NoAction,
    changed = ServerConfigDiffAction::ReloadNoRespawn,
)]
pub(crate) struct PlainTcpPortConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    #[diff(changed = ServerConfigDiffAction::ReloadAndRespawn)]
    pub(crate) listen: TcpListenConfig,
    pub(crate) listen_in_worker: bool,
    pub(crate) ingress_net_filter: Option<AclNetworkRuleBuilder>,
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
            listen_in_worker: false,
            ingress_net_filter: None,
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
            "listen_in_worker" => {
                self.listen_in_worker = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "ingress_network_filter" | "ingress_net_filter" => {
                let filter = vey_yaml::value::acl::as_ingress_network_rule_builder(v).context(
                    format!("invalid ingress network acl rule value for key {k}"),
                )?;
                self.ingress_net_filter = Some(filter);
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

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.server.is_empty() {
            return Err(anyhow!("server is not set"));
        }
        // make sure listen is always set
        self.listen.check().context("invalid listen config")?;

        Ok(())
    }
}

impl ServerConfig for PlainTcpPortConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &'static str {
        SERVER_CONFIG_TYPE
    }

    fn escaper(&self) -> &NodeName {
        Default::default()
    }

    fn user_group(&self) -> &NodeName {
        Default::default()
    }

    fn auditor(&self) -> &NodeName {
        Default::default()
    }

    fn dependent_server(&self) -> Option<BTreeSet<NodeName>> {
        let mut set = BTreeSet::new();
        set.insert(self.server.clone());
        Some(set)
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::PlainTcpPortConfig;
    use crate::config::server::{AnyServerConfig, ServerConfigDiffAction};

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
            old.diff_action(&AnyServerConfig::PlainTcpPort(new)),
            ServerConfigDiffAction::NoAction
        ));
    }
    #[test]
    fn changed_listen() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen: 127.0.0.1:18080
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen: 127.0.0.1:18081
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainTcpPort(new)),
            ServerConfigDiffAction::ReloadAndRespawn
        ));
    }
}
