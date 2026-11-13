/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use bitflags::bitflags;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_tls_ticket::TlsTicketConfig;
use vey_types::acl::AclNetworkRuleBuilder;
use vey_types::metrics::NodeName;
use vey_types::net::{QuinnEndpointConfig, RustlsServerConfigBuilder, UdpListenConfig};
use vey_yaml::YamlDocPosition;

use super::ServerConfig;
use crate::config::server::{AnyServerConfig, ServerConfigDiffAction};

const SERVER_CONFIG_TYPE: &str = "PlainQuicPort";

bitflags! {
    pub(crate) struct PlainQuicPortUpdateFlags: u64 {
        const LISTEN_CONFIG = 0b0001;
        const QUINN_CONFIG = 0b0010;
        const INGRESS_FILTER = 0b0100;
        const ACCEPT_TIMEOUT = 0b1000;
        const NEXT_SERVER = 0b0001_0000;
    }
}

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyServerConfig,
    variant = PlainQuicPort,
    mismatch = ServerConfigDiffAction::SpawnNew,
    unchanged = ServerConfigDiffAction::NoAction,
    changed = ServerConfigDiffAction::UpdateInPlace,
    flags = PlainQuicPortUpdateFlags,
)]
pub(crate) struct PlainQuicPortConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    #[diff(cmp = need_respawn, changed = ServerConfigDiffAction::ReloadAndRespawn)]
    #[diff(cmp = need_reloadable_change, flag = PlainQuicPortUpdateFlags::LISTEN_CONFIG)]
    pub(crate) listen: UdpListenConfig,
    #[diff(changed = ServerConfigDiffAction::ReloadAndRespawn)]
    pub(crate) listen_in_worker: bool,
    #[diff(cmp = ne_except_accept_timeout, flag = PlainQuicPortUpdateFlags::QUINN_CONFIG)]
    #[diff(
        cmp = accept_timeout_ne,
        flag = PlainQuicPortUpdateFlags::QUINN_CONFIG | PlainQuicPortUpdateFlags::ACCEPT_TIMEOUT
    )]
    pub(crate) tls_server: RustlsServerConfigBuilder,
    #[diff(flag = PlainQuicPortUpdateFlags::QUINN_CONFIG)]
    pub(crate) tls_ticketer: Option<TlsTicketConfig>,
    #[diff(changed = ServerConfigDiffAction::ReloadAndRespawn)]
    pub(crate) quic_endpoint: QuinnEndpointConfig,
    #[diff(flag = PlainQuicPortUpdateFlags::INGRESS_FILTER)]
    pub(crate) ingress_net_filter: Option<AclNetworkRuleBuilder>,
    #[diff(flag = PlainQuicPortUpdateFlags::NEXT_SERVER)]
    pub(crate) server: NodeName,
    pub(crate) udp_payload_max_size: Option<u16>,
}

impl PlainQuicPortConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        PlainQuicPortConfig {
            name: NodeName::default(),
            position,
            listen: UdpListenConfig::default(),
            listen_in_worker: false,
            tls_server: RustlsServerConfigBuilder::empty(),
            tls_ticketer: None,
            quic_endpoint: QuinnEndpointConfig::default(),
            ingress_net_filter: None,
            server: NodeName::default(),
            udp_payload_max_size: None,
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut server = PlainQuicPortConfig::new(position);

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
                self.listen = vey_yaml::value::as_udp_listen_config(v)
                    .context(format!("invalid udp listen config value for key {k}"))?;
                Ok(())
            }
            "listen_in_worker" => {
                self.listen_in_worker = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "tls_server" | "quic_server" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                self.tls_server =
                    vey_yaml::value::as_rustls_server_config_builder(v, Some(lookup_dir))?;
                Ok(())
            }
            "tls_ticketer" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let ticketer = TlsTicketConfig::parse_yaml(v, Some(lookup_dir))
                    .context(format!("invalid tls ticket config value for key {k}"))?;
                self.tls_ticketer = Some(ticketer);
                Ok(())
            }
            "quic_endpoint" => {
                self.quic_endpoint = vey_yaml::value::as_quinn_endpoint_config(v)
                    .context(format!("invalid quinn endpoint config for key {k}"))?;
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
        self.tls_server.check().context("invalid quic tls config")?;

        Ok(())
    }
}

impl ServerConfig for PlainQuicPortConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &'static str {
        SERVER_CONFIG_TYPE
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

    use super::PlainQuicPortConfig;
    use crate::config::server::{AnyServerConfig, ServerConfigDiffAction};

    fn parse(doc: &Yaml) -> PlainQuicPortConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        PlainQuicPortConfig::parse(
            map,
            Some(vey_yaml::YamlDocPosition {
                path: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
                index: 0,
            }),
        )
        .unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::NoAction
        ));
    }
    #[test]
    fn changed_listen() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18081
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::ReloadAndRespawn
        ));
    }
    #[test]
    fn changed_listen_socket_buffer() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
              socket_buffer:
                receive: 4096
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::UpdateInPlace(bits) if bits == (super::PlainQuicPortUpdateFlags::LISTEN_CONFIG).bits()
        ));
    }
    #[test]
    fn changed_listen_in_worker() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            listen_in_worker: true
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::ReloadAndRespawn
        ));
    }
    #[test]
    fn changed_tls_material() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              no_session_cache: true
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::UpdateInPlace(bits) if bits == (super::PlainQuicPortUpdateFlags::QUINN_CONFIG).bits()
        ));
    }
    #[test]
    fn changed_accept_timeout() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              accept_timeout: 3s
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::UpdateInPlace(bits) if bits == (super::PlainQuicPortUpdateFlags::QUINN_CONFIG | super::PlainQuicPortUpdateFlags::ACCEPT_TIMEOUT).bits()
        ));
    }
    #[test]
    fn changed_tls_ticketer() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            tls_ticketer:
              check_interval: 10s
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::UpdateInPlace(bits) if bits == (super::PlainQuicPortUpdateFlags::QUINN_CONFIG).bits()
        ));
    }
    #[test]
    fn changed_quic_endpoint() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            quic_endpoint:
              udp_payload_size: 1400
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::ReloadAndRespawn
        ));
    }
    #[test]
    fn changed_ingress_filter() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            ingress_net_filter:
              default: permit
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::UpdateInPlace(bits) if bits == (super::PlainQuicPortUpdateFlags::INGRESS_FILTER).bits()
        ));
    }
    #[test]
    fn changed_next_server() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: next
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            server: other
            listen:
              address: 127.0.0.1:18080
            tls_server:
              certificate: ../lib/vey-yaml/src/value/test_data/test_cert1.pem
              private_key: ../lib/vey-yaml/src/value/test_data/test_key1.pem
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::PlainQuicPort(Box::new(new))),
            ServerConfigDiffAction::UpdateInPlace(bits) if bits == (super::PlainQuicPortUpdateFlags::NEXT_SERVER).bits()
        ));
    }
}
