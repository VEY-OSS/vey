/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;

use anyhow::{Context, anyhow};
use bitflags::bitflags;
use yaml_rust::{Yaml, yaml};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlainQuicPortConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) listen: UdpListenConfig,
    pub(crate) listen_in_worker: bool,
    pub(crate) tls_server: RustlsServerConfigBuilder,
    pub(crate) tls_ticketer: Option<TlsTicketConfig>,
    pub(crate) quic_endpoint: QuinnEndpointConfig,
    pub(crate) ingress_net_filter: Option<AclNetworkRuleBuilder>,
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
        map: &yaml::Hash,
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

    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction {
        let AnyServerConfig::PlainQuicPort(new) = new else {
            return ServerConfigDiffAction::SpawnNew;
        };

        if self.eq(new) {
            return ServerConfigDiffAction::NoAction;
        }

        if self.listen_in_worker != new.listen_in_worker
            || self.listen.need_respawn(&new.listen)
            || self.quic_endpoint != new.quic_endpoint
        {
            return ServerConfigDiffAction::ReloadAndRespawn;
        }

        let mut flags = PlainQuicPortUpdateFlags::empty();
        if self.listen.need_reloadable_change(&new.listen) {
            flags.set(PlainQuicPortUpdateFlags::LISTEN_CONFIG, true);
        }
        if self.ingress_net_filter != new.ingress_net_filter {
            flags.set(PlainQuicPortUpdateFlags::INGRESS_FILTER, true);
        }
        if self.tls_server != new.tls_server {
            if self.tls_server.accept_timeout() != new.tls_server.accept_timeout() {
                flags.set(PlainQuicPortUpdateFlags::ACCEPT_TIMEOUT, true);
            }
            flags.set(PlainQuicPortUpdateFlags::QUINN_CONFIG, true);
        }
        if self.server != new.server {
            flags.set(PlainQuicPortUpdateFlags::NEXT_SERVER, true);
        }

        ServerConfigDiffAction::UpdateInPlace(flags.bits())
    }

    fn dependent_server(&self) -> Option<BTreeSet<NodeName>> {
        let mut set = BTreeSet::new();
        set.insert(self.server.clone());
        Some(set)
    }
}
