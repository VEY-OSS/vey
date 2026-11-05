/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::Duration;

use anyhow::anyhow;
use ip_network::IpNetwork;

use vey_histogram::HistogramMetricsConfig;
use vey_types::metrics::NodeName;
use vey_types::net::{DomainName, Host, OpensslClientConfigBuilder};
use vey_types::resolve::ResolveStrategy;

use crate::config::escaper::PeerHealthCheckConfig;

mod json;
mod yaml;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct UserSiteConfig {
    pub(crate) id: NodeName,
    pub(crate) exact_match_domain: BTreeSet<DomainName>,
    pub(crate) exact_match_ipaddr: BTreeSet<IpAddr>,
    pub(crate) subnet_match_ipaddr: BTreeSet<IpNetwork>,
    pub(crate) suffix_match_domain: BTreeSet<DomainName>,
    pub(crate) emit_stats: bool,
    pub(crate) resolve_strategy: Option<ResolveStrategy>,
    pub(crate) duration_stats: HistogramMetricsConfig,
    pub(crate) tls_client: Option<OpensslClientConfigBuilder>,
    pub(crate) http_rsp_hdr_recv_timeout: Option<Duration>,
    pub(crate) peer_health_check: Option<PeerHealthCheckConfig>,
}

impl UserSiteConfig {
    fn check(&self) -> anyhow::Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("site id is not set"));
        }
        Ok(())
    }

    fn add_exact_host(&mut self, host: Host) {
        match host {
            Host::Domain(domain) => self.exact_match_domain.insert(domain),
            Host::Ip(ip) => self.exact_match_ipaddr.insert(ip),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::UserSiteConfig;
    use saphyr::{LoadableYamlNode, Yaml};
    use serde_json::json;

    #[test]
    fn parse_peer_health_check_yaml() {
        let yaml = Yaml::load_from_str(
            r#"
id: office
exact_match:
  - www.example.com
peer_health_check:
  max_fails: 2
  fail_timeout: 15s
"#,
        )
        .unwrap();
        let site = UserSiteConfig::parse_yaml(&yaml[0], None).unwrap();
        let check = site.peer_health_check.unwrap();
        assert_eq!(check.max_fails, 2);
        assert_eq!(check.fail_timeout, std::time::Duration::from_secs(15));
    }

    #[test]
    fn parse_peer_health_check_json() {
        let value = json!({
            "id": "office",
            "exact_match": ["www.example.com"],
            "peer_health_check": { "max_fails": 2, "fail_timeout": "15s" }
        });
        let site = UserSiteConfig::parse_json(&value).unwrap();
        let check = site.peer_health_check.unwrap();
        assert_eq!(check.max_fails, 2);
        assert_eq!(check.fail_timeout, std::time::Duration::from_secs(15));
    }
}
