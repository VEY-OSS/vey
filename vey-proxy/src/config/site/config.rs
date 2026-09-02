/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_types::limit::RateLimitQuota;
use vey_types::metrics::NodeName;
use vey_types::net::{
    Host, OpensslClientConfigBuilder, OpensslServerConfigBuilder, TcpSockSpeedLimitConfig,
    UpstreamAddr,
};
use vey_yaml::{YamlDocPosition, YamlMapCallback};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SiteConfig {
    id: NodeName,
    upstream: UpstreamAddr,
    pub(crate) tls_server_builder: Option<OpensslServerConfigBuilder>,
    pub(crate) tls_client_builder: Option<OpensslClientConfigBuilder>,
    pub(crate) tls_name: Host,
    pub(crate) tcp_sock_speed_limit: TcpSockSpeedLimitConfig,
    pub(crate) request_rate_limit: Option<RateLimitQuota>,
    pub(crate) request_alive_max: Option<usize>,
}

impl Default for SiteConfig {
    fn default() -> Self {
        SiteConfig {
            id: NodeName::default(),
            upstream: UpstreamAddr::empty(),
            tls_server_builder: None,
            tls_client_builder: None,
            tls_name: Host::empty(),
            tcp_sock_speed_limit: TcpSockSpeedLimitConfig::default(),
            request_rate_limit: None,
            request_alive_max: None,
        }
    }
}

impl SiteConfig {
    pub(crate) fn id(&self) -> &NodeName {
        &self.id
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
