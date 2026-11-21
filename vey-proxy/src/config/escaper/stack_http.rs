/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

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
    HttpForwardCapability, ProxyProtocolVersion, TcpKeepAliveConfig, TcpMiscSockOpts,
    WeightedUpstreamAddr,
};
use vey_yaml::YamlDocPosition;

use super::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction, GeneralEscaperConfig};

const ESCAPER_CONFIG_TYPE: &str = "StackHttp";

#[derive(Clone, PartialEq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = StackHttp,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct StackHttpEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) next: NodeName,
    pub(crate) shared_logger: Option<AsciiString>,
    pub(crate) proxy_nodes: Vec<WeightedUpstreamAddr>,
    pub(crate) proxy_pick_policy: SelectivePickPolicy,
    proxy_username: Username,
    proxy_password: Password,
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

impl StackHttpEscaperConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        StackHttpEscaperConfig {
            name: NodeName::default(),
            position,
            next: NodeName::default(),
            shared_logger: None,
            proxy_nodes: Vec::with_capacity(1),
            proxy_pick_policy: SelectivePickPolicy::Random,
            proxy_username: Username::empty(),
            proxy_password: Password::empty(),
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

        if !self.proxy_username.is_empty() {
            if self.pass_proxy_userid {
                return Err(anyhow!(
                    "auth is needed for next proxy, we can not pass userid to it"
                ));
            }
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

impl EscaperConfig for StackHttpEscaperConfig {
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

    fn shared_logger(&self) -> Option<&str> {
        self.shared_logger.as_ref().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::StackHttpEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfigDiffAction};

    fn parse(doc: &Yaml) -> StackHttpEscaperConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        StackHttpEscaperConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            proxy_addr: 127.0.0.1:8080
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::StackHttp(new)),
            EscaperConfigDiffAction::NoAction
        ));
    }
}
