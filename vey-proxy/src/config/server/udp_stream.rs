/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ascii::AsciiString;
use bitflags::bitflags;
use saphyr::Yaml;

use vey_io_ext::LimitedUdpRelayConfig;
use vey_macros::DiffAction;
use vey_types::acl::AclNetworkRuleBuilder;
use vey_types::auth::FactsMatchType;
use vey_types::collection::SelectivePickPolicy;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{
    UdpConnectionTrackConfig, UdpListenConfig, UdpSockSpeedLimitConfig, WeightedUpstreamAddr,
};
use vey_yaml::YamlDocPosition;

use super::{
    AnyServerConfig, IDLE_CHECK_DEFAULT_DURATION, IDLE_CHECK_DEFAULT_MAX_COUNT,
    IDLE_CHECK_MAXIMUM_DURATION, ServerConfig, ServerConfigDiffAction,
};

const SERVER_CONFIG_TYPE: &str = "UdpStream";

bitflags! {
    pub(crate) struct UdpStreamServerUpdateFlags: u64 {
        const LISTEN_CONFIG = 0b0001;
        const INGRESS_FILTER = 0b0010;
    }
}

// ReloadNoRespawn replaces the server without rebinding the socket.
// The reload notification pushes the new listen options and ingress filter.
#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyServerConfig,
    variant = UdpStream,
    mismatch = ServerConfigDiffAction::SpawnNew,
    unchanged = ServerConfigDiffAction::NoAction,
    changed = ServerConfigDiffAction::ReloadNoRespawn,
)]
pub(crate) struct UdpStreamServerConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) escaper: NodeName,
    pub(crate) user_group: NodeName,
    auth_by_client_ip: bool,
    pub(crate) auth_match: Option<FactsMatchType>,
    pub(crate) shared_logger: Option<AsciiString>,
    #[diff(by = udp_listen_needs_respawn, changed = ServerConfigDiffAction::ReloadAndRespawn)]
    #[diff(by = udp_listen_reloadable_changed)]
    pub(crate) listen: Option<UdpListenConfig>,
    pub(crate) listen_in_worker: bool,
    #[diff(changed = ServerConfigDiffAction::ReloadAndRespawn)]
    pub(crate) conn_track: UdpConnectionTrackConfig,
    pub(crate) ingress_net_filter: Option<AclNetworkRuleBuilder>,
    pub(crate) upstream: Vec<WeightedUpstreamAddr>,
    pub(crate) upstream_pick_policy: SelectivePickPolicy,
    pub(crate) udp_sock_speed_limit: UdpSockSpeedLimitConfig,
    pub(crate) task_idle_check_interval: Duration,
    pub(crate) task_idle_max_count: usize,
    pub(crate) flush_task_log_on_created: bool,
    pub(crate) flush_task_log_on_connected: bool,
    pub(crate) task_log_flush_interval: Option<Duration>,
    pub(crate) udp_relay: LimitedUdpRelayConfig,
    pub(crate) extra_metrics_tags: Option<Arc<MetricTagMap>>,
}

impl UdpStreamServerConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        UdpStreamServerConfig {
            name: NodeName::default(),
            position,
            escaper: NodeName::default(),
            user_group: NodeName::default(),
            auth_by_client_ip: false,
            auth_match: None,
            shared_logger: None,
            listen: None,
            listen_in_worker: false,
            conn_track: UdpConnectionTrackConfig::default(),
            ingress_net_filter: None,
            upstream: Vec::new(),
            upstream_pick_policy: SelectivePickPolicy::Random,
            udp_sock_speed_limit: UdpSockSpeedLimitConfig::default(),
            task_idle_check_interval: IDLE_CHECK_DEFAULT_DURATION,
            task_idle_max_count: IDLE_CHECK_DEFAULT_MAX_COUNT,
            flush_task_log_on_created: false,
            flush_task_log_on_connected: false,
            task_log_flush_interval: None,
            udp_relay: LimitedUdpRelayConfig::default(),
            extra_metrics_tags: None,
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut server = Self::new(position);
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
            "escaper" => {
                self.escaper = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "user_group" => {
                self.user_group = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "auth_by_client_ip" => {
                self.auth_by_client_ip = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "shared_logger" => {
                self.shared_logger = Some(vey_yaml::value::as_ascii(v)?);
                Ok(())
            }
            "extra_metrics_tags" => {
                let tags = vey_yaml::value::as_static_metrics_tags(v)
                    .context(format!("invalid static metrics tags value for key {k}"))?;
                self.extra_metrics_tags = Some(Arc::new(tags));
                Ok(())
            }
            "listen" => {
                let config = vey_yaml::value::as_udp_listen_config(v)
                    .context(format!("invalid udp listen config value for key {k}"))?;
                self.listen = Some(config);
                Ok(())
            }
            "listen_in_worker" => {
                self.listen_in_worker = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "udp_conn_track" | "udp_connection_track" | "conn_track" | "connection_track" => {
                self.conn_track = vey_yaml::value::as_udp_conn_track_config(v).context(format!(
                    "invalid udp connection track config value for key {k}"
                ))?;
                Ok(())
            }
            "ingress_network_filter" | "ingress_net_filter" => {
                let filter = vey_yaml::value::acl::as_ingress_network_rule_builder(v).context(
                    format!("invalid ingress network acl rule value for key {k}"),
                )?;
                self.ingress_net_filter = Some(filter);
                Ok(())
            }
            "upstream" | "proxy_pass" => {
                self.upstream = vey_yaml::value::as_list(v, |v| {
                    vey_yaml::value::as_weighted_upstream_addr(v, 0)
                })
                .context(format!(
                    "invalid weighted upstream address value for key {k}"
                ))?;
                Ok(())
            }
            "upstream_pick_policy" => {
                self.upstream_pick_policy = vey_yaml::value::as_selective_pick_policy(v)?;
                Ok(())
            }
            "udp_sock_speed_limit" => {
                self.udp_sock_speed_limit = vey_yaml::value::as_udp_sock_speed_limit(v)
                    .context(format!("invalid udp socket speed limit value for key {k}"))?;
                Ok(())
            }
            "udp_relay_packet_size" => {
                let packet_size = vey_yaml::humanize::as_u16(v)
                    .context(format!("invalid humanize u16 value for key {k}"))?;
                self.udp_relay.set_packet_size(packet_size);
                Ok(())
            }
            "udp_relay_yield_count" => {
                let yield_count = vey_yaml::humanize::as_usize(v)?;
                self.udp_relay.set_yield_count(yield_count);
                Ok(())
            }
            "udp_relay_batch_count" => {
                let batch_count = vey_yaml::value::as_usize(v)?;
                self.udp_relay.set_batch_count(batch_count);
                Ok(())
            }
            "udp_relay_inderlying_buffer_size" => {
                let buffer_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                self.udp_relay.set_underlying_buffer_size(buffer_size);
                Ok(())
            }
            "task_idle_check_interval" => {
                self.task_idle_check_interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "task_idle_max_count" => {
                self.task_idle_max_count = vey_yaml::value::as_usize(v)
                    .context(format!("invalid usize value for key {k}"))?;
                Ok(())
            }
            "flush_task_log_on_created" => {
                self.flush_task_log_on_created = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "flush_task_log_on_connected" => {
                self.flush_task_log_on_connected = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "task_log_flush_interval" => {
                self.task_log_flush_interval = Some(
                    vey_yaml::humanize::as_duration(v)
                        .context(format!("invalid humanize duration value for key {k}"))?,
                );
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.escaper.is_empty() {
            return Err(anyhow!("escaper is not set"));
        }
        if self.upstream.is_empty() {
            return Err(anyhow!("upstream is not set"));
        }

        if self.auth_by_client_ip {
            self.auth_match = Some(FactsMatchType::ClientIp);
        }
        if self.auth_match.is_some() && self.user_group.is_empty() {
            return Err(anyhow!("user group is not set but auth is enabled"));
        }

        if self.task_idle_check_interval > IDLE_CHECK_MAXIMUM_DURATION {
            self.task_idle_check_interval = IDLE_CHECK_MAXIMUM_DURATION;
        }

        Ok(())
    }
}

fn udp_listen_needs_respawn(old: &Option<UdpListenConfig>, new: &Option<UdpListenConfig>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => old.need_respawn(new),
        (None, None) => false,
        _ => true,
    }
}

fn udp_listen_reloadable_changed(
    old: &Option<UdpListenConfig>,
    new: &Option<UdpListenConfig>,
) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => old.need_reloadable_change(new),
        _ => false,
    }
}

impl ServerConfig for UdpStreamServerConfig {
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
        &self.escaper
    }

    fn user_group(&self) -> &NodeName {
        &self.user_group
    }

    fn auditor(&self) -> &NodeName {
        Default::default()
    }

    fn shared_logger(&self) -> Option<&str> {
        self.shared_logger.as_ref().map(|s| s.as_str())
    }

    fn task_log_flush_interval(&self) -> Option<Duration> {
        self.task_log_flush_interval
    }

    fn task_max_idle_count(&self) -> usize {
        self.task_idle_max_count
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::UdpStreamServerConfig;
    use crate::config::server::{AnyServerConfig, ServerConfigDiffAction};

    fn parse(doc: &Yaml) -> UdpStreamServerConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        UdpStreamServerConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18080
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyServerConfig::UdpStream(new)),
            ServerConfigDiffAction::NoAction
        ));
    }
    #[test]
    fn changed_listen() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18080
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18081
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::UdpStream(new)),
            ServerConfigDiffAction::ReloadAndRespawn
        ));
    }
    #[test]
    fn changed_listen_socket_buffer() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18080
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18080
              socket_buffer:
                receive: 4096
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::UdpStream(new)),
            ServerConfigDiffAction::ReloadNoRespawn
        ));
    }
    #[test]
    fn changed_conn_track() {
        let old = parse(&yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18080
            "#
        ));
        let new = parse(&yaml_doc!(
            r#"
            name: diff-a
            escaper: esc
            upstream: 127.0.0.1:80
            listen:
              address: 127.0.0.1:18080
            conn_track:
              max_sessions: 8
            "#
        ));
        assert!(matches!(
            old.diff_action(&AnyServerConfig::UdpStream(new)),
            ServerConfigDiffAction::ReloadAndRespawn
        ));
    }
}
