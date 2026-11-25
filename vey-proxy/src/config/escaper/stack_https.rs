/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ascii::AsciiString;
use log::warn;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::auth::{Password, Username};
use vey_types::collection::SelectivePickPolicy;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{
    Host, HttpForwardCapability, OpensslClientConfigBuilder, ProxyProtocolVersion,
    TcpKeepAliveConfig, TcpMiscSockOpts, WeightedUpstreamAddr,
};
use vey_yaml::YamlDocPosition;

use super::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction, GeneralEscaperConfig};

const ESCAPER_CONFIG_TYPE: &str = "StackHttps";

#[derive(Clone, PartialEq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = StackHttps,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct StackHttpsEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) next: NodeName,
    pub(crate) shared_logger: Option<AsciiString>,
    pub(crate) proxy_nodes: Vec<WeightedUpstreamAddr>,
    pub(crate) proxy_pick_policy: SelectivePickPolicy,
    proxy_username: Username,
    proxy_password: Password,
    pub(crate) tls_config: OpensslClientConfigBuilder,
    pub(crate) tls_name: Option<Host>,
    pub(crate) general: GeneralEscaperConfig,
    pub(crate) http_forward_capability: HttpForwardCapability,
    pub(crate) tcp_keepalive: TcpKeepAliveConfig,
    pub(crate) tcp_misc_opts: TcpMiscSockOpts,
    pub(crate) http_connect_rsp_hdr_max_size: usize,
    pub(crate) pass_proxy_userid: bool,
    pub(crate) use_proxy_protocol: Option<ProxyProtocolVersion>,
    pub(crate) peer_negotiation_timeout: Duration,
    pub(crate) extra_metrics_tags: Option<Arc<MetricTagMap>>,
}

impl StackHttpsEscaperConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        StackHttpsEscaperConfig {
            name: NodeName::default(),
            position,
            next: NodeName::default(),
            shared_logger: None,
            proxy_nodes: Vec::with_capacity(1),
            proxy_pick_policy: SelectivePickPolicy::Random,
            proxy_username: Username::empty(),
            proxy_password: Password::empty(),
            tls_config: OpensslClientConfigBuilder::with_cache_for_many_sites(),
            tls_name: None,
            general: Default::default(),
            http_forward_capability: Default::default(),
            tcp_keepalive: Default::default(),
            tcp_misc_opts: Default::default(),
            http_connect_rsp_hdr_max_size: 4096,
            pass_proxy_userid: false,
            use_proxy_protocol: None,
            peer_negotiation_timeout: Duration::from_secs(10),
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
                    vey_yaml::value::as_weighted_upstream_addr(v, 3128)
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
            "tcp_conn_speed_limit" | "tcp_conn_limit" | "conn_limit" => {
                warn!("deprecated config key '{k}', please use 'tcp_sock_speed_limit' instead");
                self.set("tcp_sock_speed_limit", v)
            }
            "http_forward_capability" => {
                self.http_forward_capability = vey_yaml::value::as_http_forward_capability(v)
                    .context(format!("invalid http forward capability value for key {k}"))?;
                Ok(())
            }
            "tcp_keepalive" => {
                self.tcp_keepalive = vey_yaml::value::as_tcp_keepalive_config(v)
                    .context(format!("invalid tcp keepalive config value for key {k}"))?;
                Ok(())
            }
            "tcp_misc_opts" => {
                self.tcp_misc_opts = vey_yaml::value::as_tcp_misc_sock_opts(v)
                    .context(format!("invalid tcp misc sock opts value for key {k}"))?;
                Ok(())
            }
            "tls" | "tls_client" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                self.tls_config = vey_yaml::value::as_to_many_openssl_tls_client_config_builder(
                    v,
                    Some(lookup_dir),
                )
                .context(format!(
                    "invalid openssl tls client config value for key {k}"
                ))?;
                Ok(())
            }
            "tls_name" => {
                let name = vey_yaml::value::as_host(v)
                    .context(format!("invalid tls server name value for key {k}"))?;
                self.tls_name = Some(name);
                Ok(())
            }
            "tcp_connect" => {
                self.general.tcp_connect = vey_yaml::value::as_tcp_connect_config(v)
                    .context(format!("invalid tcp connect value for key {k}"))?;
                Ok(())
            }
            "http_connect_rsp_header_max_size" => {
                self.http_connect_rsp_hdr_max_size = vey_yaml::humanize::as_usize(v)
                    .context(format!("invalid humanize usize value for key {k}"))?;
                Ok(())
            }
            "pass_proxy_userid" => {
                self.pass_proxy_userid = vey_yaml::value::as_bool(v)
                    .context(format!("invalid bool value for key {k}"))?;
                Ok(())
            }
            "use_proxy_protocol" => {
                let version = vey_yaml::value::as_proxy_protocol_version(v)
                    .context(format!("invalid ProxyProtocolVersion value for key {k}"))?;
                self.use_proxy_protocol = Some(version);
                Ok(())
            }
            "peer_negotiation_timeout" => {
                self.peer_negotiation_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
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

        if !self.proxy_username.is_empty() && self.pass_proxy_userid {
            return Err(anyhow!(
                "auth is needed for next proxy, we can not pass userid to it"
            ));
        }

        Ok(())
    }

    pub(crate) fn proxy_http_headers(&self) -> Vec<String> {
        let mut headers = Vec::new();
        if !self.proxy_username.is_empty() {
            let auth = vey_http::header::proxy_authorization_basic(
                &self.proxy_username,
                &self.proxy_password,
            );
            headers.push(auth);
        }
        headers
    }
}

impl EscaperConfig for StackHttpsEscaperConfig {
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
    use std::str::FromStr;
    use std::time::Duration;

    use http::Method;
    use saphyr::Yaml;

    use vey_types::collection::SelectivePickPolicy;
    use vey_types::metrics::MetricTagName;
    use vey_types::net::{OpensslClientConfigBuilder, ProxyProtocolVersion};
    use vey_yaml::{YamlDocPosition, yaml_doc};

    use super::StackHttpsEscaperConfig;
    use crate::config::escaper::dummy_deny::DummyDenyEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction};

    fn parse(yaml: &str) -> anyhow::Result<StackHttpsEscaperConfig> {
        let doc = yaml_doc!(yaml);
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        StackHttpsEscaperConfig::parse(&map, None)
    }

    fn parse_ok(yaml: &str) -> StackHttpsEscaperConfig {
        parse(yaml).unwrap()
    }

    fn parse_err(yaml: &str) -> String {
        match parse(yaml) {
            Ok(_) => panic!("expected parse error"),
            Err(err) => format!("{err:#}"),
        }
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
        assert_eq!(config.r#type(), "StackHttps");
        assert!(config.resolver().is_empty());
        assert!(config.position().is_none());
        assert!(config.shared_logger().is_none());
        assert!(config.tls_name.is_none());
        let next = config.dependent_escaper().unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next.iter().next().unwrap().as_str(), "direct");
        assert_eq!(config.proxy_nodes.len(), 1);
        assert_eq!(config.proxy_nodes[0].inner().port(), 3128);
        assert_eq!(config.proxy_pick_policy, SelectivePickPolicy::Random);
        assert!(config.proxy_http_headers().is_empty());
        assert!(!config.pass_proxy_userid);
        assert!(config.use_proxy_protocol.is_none());
        assert_eq!(config.http_connect_rsp_hdr_max_size, 4096);
        assert_eq!(config.peer_negotiation_timeout, Duration::from_secs(10));
        assert!(!config.http_forward_capability.forward_https());
    }

    #[test]
    fn full_yaml_covers_keys_and_aliases() {
        let config = parse_ok(
            r#"
            type: stack_https
            name: stack-a
            next: direct
            shared_logger: escape
            extra_metrics_tags:
              pool: edge
            proxy_addr:
              - 127.0.0.1:1001
              - 127.0.0.1:1002
            proxy_addr_pick_policy: serial
            proxy_username: user
            proxy_password: pass
            tcp_sock_speed_limit:
              shift: 5
              upload: 5MB
              download: 10MB
            tcp_connect:
              max_retry: 5
              each_timeout: 10s
            tls_name: proxy.local
            http_forward_capability:
              forward_https: true
              forward_ftp_get: true
            tcp_keepalive: true
            tcp_misc_opts:
              no_delay: false
              max_segment_size: 1460
            http_connect_rsp_header_max_size: 2048
            use_proxy_protocol: 1
            peer_negotiation_timeout: 1500ms
            "#,
        );
        assert_eq!(config.shared_logger(), Some("escape"));
        assert_eq!(config.extra_metrics_tags.as_ref().unwrap().len(), 1);
        assert_eq!(
            config
                .extra_metrics_tags
                .as_ref()
                .unwrap()
                .get(&MetricTagName::from_str("pool").unwrap())
                .unwrap()
                .as_str(),
            "edge"
        );
        assert_eq!(config.proxy_nodes[0].inner().port(), 1002);
        assert_eq!(config.proxy_nodes[1].inner().port(), 1001);
        assert_eq!(config.proxy_pick_policy, SelectivePickPolicy::Serial);
        assert_eq!(
            config.proxy_http_headers(),
            vec!["Proxy-Authorization: Basic dXNlcjpwYXNz\r\n".to_string()]
        );
        assert_eq!(config.general.tcp_sock_speed_limit.shift_millis, 5);
        assert_eq!(config.general.tcp_sock_speed_limit.max_north, 5_000_000);
        assert_eq!(config.general.tcp_sock_speed_limit.max_south, 10_000_000);
        assert_eq!(config.general.tcp_connect.max_tries(), 6);
        assert_eq!(
            config.general.tcp_connect.each_timeout(),
            Duration::from_secs(10)
        );
        assert_eq!(config.tls_name.as_ref().unwrap().to_string(), "proxy.local");
        assert!(config.http_forward_capability.forward_https());
        assert!(config.http_forward_capability.forward_ftp(&Method::GET));
        assert!(!config.http_forward_capability.forward_ftp(&Method::PUT));
        assert!(config.tcp_keepalive.is_enabled());
        assert_eq!(config.tcp_misc_opts.no_delay, Some(false));
        assert_eq!(config.tcp_misc_opts.max_segment_size, Some(1460));
        assert_eq!(config.http_connect_rsp_hdr_max_size, 2048);
        assert_eq!(config.use_proxy_protocol, Some(ProxyProtocolVersion::V1));
        assert_eq!(config.peer_negotiation_timeout, Duration::from_millis(1500));

        let deprecated = parse_ok(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            tcp_conn_limit: 2MB
            "#,
        );
        assert_eq!(deprecated.general.tcp_sock_speed_limit.max_north, 2_000_000);
    }

    #[test]
    fn reject_incomplete_or_conflicting_yaml() {
        assert!(
            parse_err(
                r#"
            next: direct
            proxy_addr: 127.0.0.1:8080
            "#
            )
            .contains("name is not set")
        );
        assert!(
            parse_err(
                r#"
            name: stack-a
            proxy_addr: 127.0.0.1:8080
            "#
            )
            .contains("next escaper is not set")
        );
        assert!(
            parse_err(
                r#"
            name: stack-a
            next: direct
            "#
            )
            .contains("proxy addr is not set")
        );
        assert!(
            parse_err(
                r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            proxy_user: user
            pass_proxy_userid: true
            "#
            )
            .contains("can not pass userid")
        );
        assert!(
            parse_err(
                r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            resolver: main
            "#
            )
            .contains("invalid key")
        );

        let tls_without_dir = parse(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            tls_client: {}
            "#,
        );
        assert!(tls_without_dir.is_err());

        let bad_tls_name = parse(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            tls_name: ""
            "#,
        );
        assert!(bad_tls_name.is_err());

        let bad_timeout = parse(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            peer_negotiation_timeout: later
            "#,
        );
        assert!(bad_timeout.is_err());

        let position = YamlDocPosition {
            path: std::env::temp_dir().join("stack-https-escaper.yaml"),
            index: 0,
        };
        let doc = yaml_doc!(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8080
            tls_client: 1
            "#
        );
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        assert!(StackHttpsEscaperConfig::parse(&map, Some(position)).is_err());
    }

    #[test]
    fn tls_client_uses_config_lookup_dir() {
        let position = YamlDocPosition {
            path: std::env::temp_dir().join("stack-https-escaper.yaml"),
            index: 0,
        };
        let doc = yaml_doc!(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8443
            tls:
              insecure: true
              handshake_timeout: 10s
            "#
        );
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        let config = StackHttpsEscaperConfig::parse(&map, Some(position.clone())).unwrap();
        assert_eq!(config.position(), Some(position));
        let mut expected = OpensslClientConfigBuilder::with_cache_for_many_sites();
        expected.set_insecure(true);
        expected.set_handshake_timeout(Duration::from_secs(10));
        assert_eq!(config.tls_config, expected);
    }

    #[test]
    fn diff_action() {
        let yaml = r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8443
            tls_name: proxy.local
        "#;
        let old = parse_ok(yaml);
        let same = parse_ok(yaml);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::StackHttps(same)),
            EscaperConfigDiffAction::NoAction
        ));

        let changed = parse_ok(
            r#"
            name: stack-a
            next: direct
            proxy_addr: 127.0.0.1:8443
            tls_name: other.local
            "#,
        );
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::StackHttps(changed)),
            EscaperConfigDiffAction::Reload
        ));

        let other = DummyDenyEscaperConfig::new(None, None);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::DummyDeny(other)),
            EscaperConfigDiffAction::SpawnNew
        ));
    }
}
