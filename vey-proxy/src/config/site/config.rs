/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeMap;
use std::str::FromStr;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_dpi::MaybeProtocol;
use vey_types::limit::RateLimitQuota;
use vey_types::metrics::NodeName;
use vey_types::net::{
    Host, OpensslClientConfigBuilder, OpensslServerConfigBuilder, TcpConnectConfig,
    TcpKeepAliveConfig, TcpMiscSockOpts, TcpSockSpeedLimitConfig, UdpMiscSockOpts, UpstreamAddr,
};
use vey_types::resolve::ResolveStrategy;
use vey_yaml::{YamlDocPosition, YamlMapCallback};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SiteConfig {
    id: NodeName,
    /// The tenant this site belongs to. Looked up in the site group's
    /// `tenant_user_group` and attached as `SiteContext` tenant. Reported as
    /// `-` in site metrics when unset.
    owner: NodeName,
    upstream: UpstreamAddr,
    pub(crate) tls_server_builder: Option<OpensslServerConfigBuilder>,
    pub(crate) tls_client_builder: Option<OpensslClientConfigBuilder>,
    pub(crate) tls_name: Host,
    /// Inner protocol after TLS termination. Used by `tls_proxy` DPI only.
    pub(crate) dpi_protocol: Option<MaybeProtocol>,
    pub(crate) tcp_sock_speed_limit: TcpSockSpeedLimitConfig,
    pub(crate) request_rate_limit: Option<RateLimitQuota>,
    pub(crate) request_alive_max: Option<usize>,
    pub(crate) task_idle_max_count: Option<usize>,
    pub(crate) resolve_strategy: Option<ResolveStrategy>,
    pub(crate) tcp_connect: Option<TcpConnectConfig>,
    pub(crate) tcp_remote_keepalive: TcpKeepAliveConfig,
    pub(crate) tcp_remote_misc_opts: Option<TcpMiscSockOpts>,
    pub(crate) udp_remote_misc_opts: Option<UdpMiscSockOpts>,
    pub(crate) egress_path_id_map: BTreeMap<NodeName, String>,
    pub(crate) egress_path_value_map: BTreeMap<NodeName, serde_json::Value>,
}

impl Default for SiteConfig {
    fn default() -> Self {
        SiteConfig {
            id: NodeName::default(),
            owner: NodeName::default(),
            upstream: UpstreamAddr::empty(),
            tls_server_builder: None,
            tls_client_builder: None,
            tls_name: Host::empty(),
            dpi_protocol: None,
            tcp_sock_speed_limit: TcpSockSpeedLimitConfig::default(),
            request_rate_limit: None,
            request_alive_max: None,
            task_idle_max_count: None,
            resolve_strategy: None,
            tcp_connect: None,
            tcp_remote_keepalive: TcpKeepAliveConfig::default(),
            tcp_remote_misc_opts: None,
            udp_remote_misc_opts: None,
            egress_path_id_map: BTreeMap::new(),
            egress_path_value_map: BTreeMap::new(),
        }
    }
}

impl SiteConfig {
    pub(crate) fn id(&self) -> &NodeName {
        &self.id
    }

    pub(crate) fn owner(&self) -> &NodeName {
        &self.owner
    }

    pub(crate) fn upstream(&self) -> &UpstreamAddr {
        &self.upstream
    }
}

impl YamlMapCallback for SiteConfig {
    fn type_name(&self) -> &'static str {
        "SiteConfig"
    }

    fn parse_kv(
        &mut self,
        key: &str,
        value: &Yaml,
        doc: Option<&YamlDocPosition>,
    ) -> anyhow::Result<()> {
        match key {
            "id" | "name" => {
                self.id = vey_yaml::value::as_metric_node_name(value)?;
                Ok(())
            }
            "owner" | "tenant" => {
                self.owner = vey_yaml::value::as_metric_node_name(value)
                    .context(format!("invalid metric node name value for key {key}"))?;
                Ok(())
            }
            "upstream" => {
                self.upstream = vey_yaml::value::as_upstream_addr(value, 80)
                    .context(format!("invalid upstream addr value for key {key}"))?;
                Ok(())
            }
            "tls_server" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(doc)?;
                let builder =
                    vey_yaml::value::as_openssl_tls_server_config_builder(value, Some(lookup_dir))
                        .context(format!(
                            "invalid openssl tls server config builder value for key {key}"
                        ))?;
                self.tls_server_builder = Some(builder);
                Ok(())
            }
            "tls_client" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(doc)?;
                let builder = vey_yaml::value::as_to_one_openssl_tls_client_config_builder(
                    value,
                    Some(lookup_dir),
                )
                .context(format!(
                    "invalid openssl tls client config value for key {key}"
                ))?;
                self.tls_client_builder = Some(builder);
                Ok(())
            }
            "tls_name" => {
                self.tls_name = vey_yaml::value::as_host(value)
                    .context(format!("invalid tls name value for key {key}"))?;
                Ok(())
            }
            "dpi_protocol" => {
                let protocol = vey_yaml::value::as_string(value)
                    .context(format!("invalid protocol string value for key {key}"))?;
                self.dpi_protocol = Some(
                    MaybeProtocol::from_str(&protocol)
                        .map_err(|_| anyhow!("unrecognised dpi_protocol {protocol}"))?,
                );
                Ok(())
            }
            "tcp_sock_speed_limit" => {
                self.tcp_sock_speed_limit = vey_yaml::value::as_tcp_sock_speed_limit(value)
                    .context(format!(
                        "invalid tcp socket speed limit value for key {key}"
                    ))?;
                Ok(())
            }
            "request_rate_limit" | "request_limit_quota" => {
                let quota = vey_yaml::value::as_rate_limit_quota(value)
                    .context(format!("invalid request quota value for key {key}"))?;
                self.request_rate_limit = Some(quota);
                Ok(())
            }
            "request_max_alive" | "request_alive_max" => {
                let alive_max = vey_yaml::value::as_usize(value)
                    .context(format!("invalid usize value for key {key}"))?;
                self.request_alive_max = Some(alive_max);
                Ok(())
            }
            "task_idle_max_count" => {
                let count = vey_yaml::value::as_usize(value)
                    .context(format!("invalid usize value for key {key}"))?;
                self.task_idle_max_count = Some(count);
                Ok(())
            }
            "resolve_strategy" => {
                self.resolve_strategy = Some(
                    vey_yaml::value::as_resolve_strategy(value)
                        .context(format!("invalid resolve strategy value for key {key}"))?,
                );
                Ok(())
            }
            "tcp_connect" => {
                self.tcp_connect = Some(
                    vey_yaml::value::as_tcp_connect_config(value)
                        .context(format!("invalid tcp connect config value for key {key}"))?,
                );
                Ok(())
            }
            "tcp_remote_keepalive" => {
                self.tcp_remote_keepalive = vey_yaml::value::as_tcp_keepalive_config(value)
                    .context(format!("invalid tcp keepalive config value for key {key}"))?;
                Ok(())
            }
            "tcp_remote_misc_opts" => {
                self.tcp_remote_misc_opts = Some(
                    vey_yaml::value::as_tcp_misc_sock_opts(value)
                        .context(format!("invalid tcp misc sock opts value for key {key}"))?,
                );
                Ok(())
            }
            "udp_remote_misc_opts" => {
                self.udp_remote_misc_opts = Some(
                    vey_yaml::value::as_udp_misc_sock_opts(value)
                        .context(format!("invalid udp misc sock opts value for key {key}"))?,
                );
                Ok(())
            }
            "egress_path_id_map" => {
                self.egress_path_id_map = vey_yaml::value::as_hashmap(
                    value,
                    vey_yaml::value::as_metric_node_name,
                    vey_yaml::value::as_string,
                )
                .context(format!("invalid egress path id map value for key {key}"))?
                .into_iter()
                .collect();
                Ok(())
            }
            "egress_path_value_map" => {
                self.egress_path_value_map =
                    vey_yaml::value::as_hashmap(value, vey_yaml::value::as_metric_node_name, |v| {
                        let v = vey_yaml::value::as_string(v)?;
                        serde_json::Value::from_str(&v)
                            .map_err(|e| anyhow!("invalid json string: {e}"))
                    })
                    .context(format!("invalid egress path value map value for key {key}"))?
                    .into_iter()
                    .collect();
                Ok(())
            }
            _ => Err(anyhow!("invalid key {key}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("id is not set"));
        }
        if self.upstream.is_empty() {
            return Err(anyhow!("upstream is empty"));
        }
        if self.tls_name.is_empty() {
            self.upstream.host().clone_into(&mut self.tls_name);
        }
        Ok(())
    }
}
