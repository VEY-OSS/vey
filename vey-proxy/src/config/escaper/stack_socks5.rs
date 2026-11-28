/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ascii::AsciiString;
use rustc_hash::FxHashMap;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::auth::{Password, Username};
use vey_types::collection::SelectivePickPolicy;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{SocksAuth, WeightedUpstreamAddr};
use vey_yaml::YamlDocPosition;

use super::{
    AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction, GeneralEscaperConfig,
    PeerHealthCheckConfig,
};

const ESCAPER_CONFIG_TYPE: &str = "StackSocks5";

#[derive(Clone, PartialEq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = StackSocks5,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct StackSocks5EscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) next: NodeName,
    pub(crate) shared_logger: Option<AsciiString>,
    pub(crate) proxy_nodes: Vec<WeightedUpstreamAddr>,
    pub(crate) proxy_pick_policy: SelectivePickPolicy,
    proxy_username: Username,
    proxy_password: Password,
    pub(crate) general: GeneralEscaperConfig,
    pub(crate) auth_info: SocksAuth,
    pub(crate) peer_negotiation_timeout: Duration,
    pub(crate) peer_health_check: Option<PeerHealthCheckConfig>,
    transmute_udp_peer_ip: Option<FxHashMap<IpAddr, IpAddr>>,
    pub(crate) end_on_control_closed: bool,
    pub(crate) extra_metrics_tags: Option<Arc<MetricTagMap>>,
}

impl StackSocks5EscaperConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        StackSocks5EscaperConfig {
            name: NodeName::default(),
            position,
            next: NodeName::default(),
            shared_logger: None,
            proxy_nodes: Vec::with_capacity(1),
            proxy_pick_policy: SelectivePickPolicy::Random,
            proxy_username: Username::empty(),
            proxy_password: Password::empty(),
            general: Default::default(),
            auth_info: SocksAuth::None,
            peer_negotiation_timeout: Duration::from_secs(10),
            peer_health_check: None,
            transmute_udp_peer_ip: None,
            end_on_control_closed: false,
            extra_metrics_tags: None,
        }
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut config = Self::new(position);

        vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;

        config.check()?;
        Ok(config)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_ESCAPER_TYPE => Ok(()),
            super::CONFIG_KEY_ESCAPER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "next" => {
                self.next = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "shared_logger" => {
                let name = vey_yaml::value::as_ascii(v)?;
                self.shared_logger = Some(name);
                Ok(())
            }
            "extra_metrics_tags" => {
                let tags = vey_yaml::value::as_static_metrics_tags(v)
                    .context(format!("invalid static metrics tags value for key {k}"))?;
                self.extra_metrics_tags = Some(Arc::new(tags));
                Ok(())
            }
            "proxy_addr" => {
                self.proxy_nodes = vey_yaml::value::as_list(v, |v| {
                    vey_yaml::value::as_weighted_upstream_addr(v, 1080)
                })
                .context(format!(
                    "invalid weighted upstream address list value for key {k}"
                ))?;
                Ok(())
            }
            "proxy_addr_pick_policy" => {
                self.proxy_pick_policy = vey_yaml::value::as_selective_pick_policy(v)?;
                Ok(())
            }
            "proxy_username" | "proxy_user" => {
                self.proxy_username = vey_yaml::value::as_username(v)
                    .context(format!("invalid username value for key {k}"))?;
                Ok(())
            }
            "proxy_password" | "proxy_passwd" => {
                self.proxy_password = vey_yaml::value::as_password(v)
                    .context(format!("invalid password value for key {k}"))?;
                Ok(())
            }
            "tcp_sock_speed_limit" => {
                self.general.tcp_sock_speed_limit = vey_yaml::value::as_tcp_sock_speed_limit(v)
                    .context(format!("invalid tcp socket speed limit value for key {k}"))?;
                Ok(())
            }
            "udp_sock_speed_limit" => {
                self.general.udp_sock_speed_limit = vey_yaml::value::as_udp_sock_speed_limit(v)
                    .context(format!("invalid udp socket speed limit value for key {k}"))?;
                Ok(())
            }
            "peer_negotiation_timeout" => {
                self.peer_negotiation_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "peer_health_check" => {
                let health_check = PeerHealthCheckConfig::parse(v).context(format!(
                    "invalid peer health check config value for key {k}"
                ))?;
                self.peer_health_check = Some(health_check);
                Ok(())
            }
            "transmute_udp_peer_ip" => {
                if let Yaml::Mapping(_) = v {
                    let map = vey_yaml::value::as_hashmap(
                        v,
                        vey_yaml::value::as_ipaddr,
                        vey_yaml::value::as_ipaddr,
                    )
                    .context(format!("invalid IP:IP hashmap value for key {k}"))?;
                    self.transmute_udp_peer_ip = Some(map.into_iter().collect::<FxHashMap<_, _>>());
                } else {
                    let enable = vey_yaml::value::as_bool(v)?;
                    if enable {
                        self.transmute_udp_peer_ip = Some(FxHashMap::default());
                    }
                }
                Ok(())
            }
            "end_on_control_closed" => {
                self.end_on_control_closed = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.next.is_empty() {
            return Err(anyhow!("next escaper is not set"));
        }
        if self.proxy_nodes.is_empty() {
            return Err(anyhow!("proxy addr is not set"));
        }
        self.proxy_nodes.reverse(); // reverse as we push to the back

        if !self.proxy_username.is_empty() {
            self.auth_info =
                SocksAuth::User(self.proxy_username.clone(), self.proxy_password.clone());
        }

        Ok(())
    }

    pub(crate) fn transmute_udp_peer_addr(
        &self,
        returned_addr: SocketAddr,
        tcp_peer_ip: IpAddr,
    ) -> SocketAddr {
        if let Some(map) = &self.transmute_udp_peer_ip {
            let ip = map.get(&returned_addr.ip()).copied().unwrap_or(tcp_peer_ip);
            SocketAddr::new(ip, returned_addr.port())
        } else if returned_addr.ip().is_unspecified() {
            SocketAddr::new(tcp_peer_ip, returned_addr.port())
        } else {
            returned_addr
        }
    }
}

impl EscaperConfig for StackSocks5EscaperConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &str {
        ESCAPER_CONFIG_TYPE
    }

    fn resolver(&self) -> &NodeName {
        Default::default()
    }

    fn dependent_escaper(&self) -> Option<BTreeSet<NodeName>> {
        let mut set = BTreeSet::new();
        set.insert(self.next.clone());
        Some(set)
    }

    fn shared_logger(&self) -> Option<&str> {
        self.shared_logger.as_ref().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::time::Duration;

    use saphyr::Yaml;

    use vey_types::collection::SelectivePickPolicy;
    use vey_types::net::SocksAuth;
    use vey_yaml::yaml_doc;

    use super::StackSocks5EscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction};

    fn parse(yaml: &str) -> anyhow::Result<StackSocks5EscaperConfig> {
        let doc = yaml_doc!(yaml);
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        StackSocks5EscaperConfig::parse(&map, None)
    }

    fn parse_ok(yaml: &str) -> StackSocks5EscaperConfig {
        parse(yaml).unwrap()
    }

    #[test]
    fn minimal_yaml() {
        let config = parse_ok(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1
            "#,
        );
        assert_eq!(config.name().as_str(), "stack-a");
        assert_eq!(config.r#type(), "StackSocks5");
        assert!(config.resolver().is_empty());
        assert!(config.shared_logger().is_none());
        let next = config.dependent_escaper().unwrap();
        assert_eq!(next.iter().next().unwrap().as_str(), "direct");
        assert_eq!(config.proxy_nodes[0].inner().port(), 1080);
        assert_eq!(config.proxy_pick_policy, SelectivePickPolicy::Random);
        assert!(matches!(config.auth_info, SocksAuth::None));
        assert_eq!(config.peer_negotiation_timeout, Duration::from_secs(10));
        assert!(config.peer_health_check.is_none());
        assert!(!config.end_on_control_closed);
        assert!(config.transmute_udp_peer_ip.is_none());
    }

    #[test]
    fn auth_alias_and_udp_peer_rewrite() {
        let config = parse_ok(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:1080
            proxy_user: user
            proxy_passwd: secret
            end_on_control_closed: true
            transmute_udp_peer_ip: true
            "#,
        );
        assert!(matches!(config.auth_info, SocksAuth::User(_, _)));
        let returned = SocketAddr::from((Ipv4Addr::UNSPECIFIED, 9));
        let tcp_peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        assert_eq!(
            config.transmute_udp_peer_addr(returned, tcp_peer),
            SocketAddr::from((tcp_peer, 9))
        );
        assert!(config.end_on_control_closed);
    }

    #[test]
    fn socket_options_belong_to_next() {
        let err = match parse(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:1080
            tcp_keepalive: true
            "#,
        ) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("tcp_keepalive"));
    }

    #[test]
    fn same_config_reloads_only_when_changed() {
        let yaml = r#"
            name: diff-a
            next: direct
            proxy_addr: 127.0.0.1:1080
        "#;
        let old = parse_ok(yaml);
        let same = parse_ok(yaml);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::StackSocks5(same)),
            EscaperConfigDiffAction::NoAction
        ));

        let changed = parse_ok(
            r#"
            name: diff-a
            next: other
            proxy_addr: 127.0.0.1:1080
            "#,
        );
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::StackSocks5(changed)),
            EscaperConfigDiffAction::Reload
        ));
    }
}
