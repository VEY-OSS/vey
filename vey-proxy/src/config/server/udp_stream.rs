/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ascii::AsciiString;
use log::warn;
use yaml_rust::{Yaml, yaml};

use vey_io_ext::LimitedUdpRelayConfig;
use vey_types::acl::AclNetworkRuleBuilder;
use vey_types::auth::FactsMatchType;
use vey_types::collection::SelectivePickPolicy;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{SocketBufferConfig, UdpListenConfig, WeightedUpstreamAddr};
use vey_yaml::YamlDocPosition;

use super::{
    AnyServerConfig, IDLE_CHECK_DEFAULT_DURATION, IDLE_CHECK_DEFAULT_MAX_COUNT,
    IDLE_CHECK_MAXIMUM_DURATION, ServerConfig, ServerConfigDiffAction,
};

const SERVER_CONFIG_TYPE: &str = "UdpStream";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UdpStreamServerConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) escaper: NodeName,
    pub(crate) user_group: NodeName,
    auth_by_client_ip: bool,
    pub(crate) auth_match: Option<FactsMatchType>,
    pub(crate) shared_logger: Option<AsciiString>,
    pub(crate) listen: Option<UdpListenConfig>,
    pub(crate) listen_in_worker: bool,
    pub(crate) ingress_net_filter: Option<AclNetworkRuleBuilder>,
    pub(crate) upstream: Vec<WeightedUpstreamAddr>,
    pub(crate) upstream_pick_policy: SelectivePickPolicy,
    pub(crate) udp_socket_buffer: SocketBufferConfig,
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
            ingress_net_filter: None,
            upstream: Vec::new(),
            upstream_pick_policy: SelectivePickPolicy::Random,
            udp_socket_buffer: SocketBufferConfig::default(),
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
        map: &yaml::Hash,
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
            "udp_socket_buffer" => {
                self.udp_socket_buffer = vey_yaml::value::as_socket_buffer_config(v)
                    .context(format!("invalid socket buffer config value for key {k}"))?;
                Ok(())
            }
            "udp_relay_packet_size" => {
                let packet_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
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
            "task_idle_check_duration" => {
                warn!("deprecated config key '{k}', please use 'task_idle_check_interval' instead");
                self.set("task_idle_check_interval", v)
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

    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction {
        let AnyServerConfig::UdpStream(new) = new else {
            return ServerConfigDiffAction::SpawnNew;
        };

        if self.eq(new) {
            return ServerConfigDiffAction::NoAction;
        }

        if self.listen != new.listen {
            return ServerConfigDiffAction::ReloadAndRespawn;
        }

        ServerConfigDiffAction::ReloadNoRespawn
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
