/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use ip_network::IpNetwork;
use ip_network_table::IpNetworkTable;
use yaml_rust::Yaml;

use vey_types::net::{ConnectionPoolConfig, HttpForwardedHeaderType, HttpKeepAliveConfig};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteHttpH1Config {
    /// HTTP/1 origin idle pool. `None` returns idle connections to the
    /// per-pipeline forward context instead.
    pub(crate) connection_pool: Option<ConnectionPoolConfig>,
    /// Reuse idle HTTP/1 origin connections. Enabled by default.
    pub(crate) upstream_keepalive: HttpKeepAliveConfig,
}

impl SiteHttpH1Config {
    fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!(
                "yaml value type for 'site http h1' should be 'map'"
            ));
        };
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "connection_pool" => {
                let pool = vey_yaml::value::as_connection_pool_config(v)
                    .context(format!("invalid connection pool config for key {k}"))?;
                self.connection_pool = Some(pool);
                Ok(())
            }
            "upstream_keepalive" => {
                self.upstream_keepalive = vey_yaml::value::as_http_keepalive_config(v)
                    .context(format!("invalid http keepalive config value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteHttpH2Config {
    /// HTTP/2 origin multiplex pool. Always on: H2 streams are not bound 1:1.
    pub(crate) connection_pool: ConnectionPoolConfig,
    pub(crate) ping_interval: Duration,
    pub(crate) ping_timeout: Duration,
}

impl Default for SiteHttpH2Config {
    fn default() -> Self {
        Self {
            connection_pool: ConnectionPoolConfig::default(),
            ping_interval: Duration::from_secs(60),
            ping_timeout: Duration::from_secs(1),
        }
    }
}

impl SiteHttpH2Config {
    fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!(
                "yaml value type for 'site http h2' should be 'map'"
            ));
        };
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "connection_pool" => {
                self.connection_pool = vey_yaml::value::as_connection_pool_config(v)
                    .context(format!("invalid connection pool config for key {k}"))?;
                Ok(())
            }
            "ping_interval" => {
                self.ping_interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "ping_timeout" => {
                self.ping_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteHttpConfig {
    pub(crate) rsp_hdr_recv_timeout: Option<Duration>,
    /// How originating-client identity is written on origin requests.
    pub(crate) forwarded_header_type: HttpForwardedHeaderType,
    /// Immediate client addresses allowed to keep inbound `Forwarded` / `X-Forwarded-*`.
    /// Empty / unset means inbound forwarded headers are discarded.
    pub(crate) forwarded_trusted_from: Vec<IpNetwork>,
    pub(crate) h1: SiteHttpH1Config,
    pub(crate) h2: SiteHttpH2Config,
}

impl SiteHttpConfig {
    pub(crate) fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("yaml value type for 'site http' should be 'map'"));
        };
        vey_yaml::foreach_kv(map, |k, v| self.set(k, v))
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "rsp_header_recv_timeout" => {
                let timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                self.rsp_hdr_recv_timeout = Some(timeout);
                Ok(())
            }
            "forwarded_header_type" => {
                self.forwarded_header_type = vey_yaml::value::as_http_forwarded_header_type(v)
                    .context(format!(
                        "invalid http forwarded header type value for key {k}"
                    ))?;
                Ok(())
            }
            "forwarded_trusted_from" | "x_forwarded_for_trusted" => {
                self.forwarded_trusted_from =
                    vey_yaml::value::as_list(v, vey_yaml::value::as_ip_network).context(
                        format!("invalid forwarded trusted-from network list for key {k}"),
                    )?;
                Ok(())
            }
            "h1" => self.h1.parse_yaml(v),
            "h2" => self.h2.parse_yaml(v),
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    pub(crate) fn check(&mut self) {
        self.forwarded_trusted_from.sort_unstable();
    }

    pub(crate) fn build_forwarded_trusted_from_table(&self) -> IpNetworkTable<()> {
        let mut table = IpNetworkTable::new();
        for net in &self.forwarded_trusted_from {
            table.insert(*net, ());
        }
        table
    }
}
