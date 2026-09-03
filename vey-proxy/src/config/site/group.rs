/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;
use vey_types::route::HostMatch;
use vey_yaml::YamlDocPosition;

use super::SiteConfig;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SiteGroupConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    tenant_user_group: NodeName,
    pub(crate) sites: HostMatch<Arc<SiteConfig>>,
}

impl SiteGroupConfig {
    pub(crate) fn name(&self) -> &NodeName {
        &self.name
    }

    pub(crate) fn tenant_user_group(&self) -> &NodeName {
        &self.tenant_user_group
    }

    pub(crate) fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    pub(crate) fn empty(name: &NodeName) -> Self {
        SiteGroupConfig {
            name: name.clone(),
            position: None,
            tenant_user_group: NodeName::default(),
            sites: HostMatch::default(),
        }
    }

    fn new(position: Option<YamlDocPosition>) -> Self {
        SiteGroupConfig {
            name: NodeName::default(),
            position,
            tenant_user_group: NodeName::default(),
            sites: HostMatch::default(),
        }
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut group = SiteGroupConfig::new(position);
        vey_yaml::foreach_kv(map, |k, v| group.set(k, v))?;
        group.check()?;
        Ok(group)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "name" => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "tenant_user_group" => {
                self.tenant_user_group = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "static_sites" | "sites" => {
                self.sites = vey_yaml::value::as_host_matched_obj(v, self.position.as_ref())
                    .context(format!("invalid host matched site value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use vey_types::metrics::NodeName;
    use vey_types::net::Host;
    use yaml_rust::YamlLoader;

    #[test]
    fn parse_static_sites() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
tenant_user_group: customers
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
  - id: unowned
    exact_match: other.internal
    upstream: 127.0.0.1:8081
    owner: team_a
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        assert_eq!(group.name().as_str(), "local");
        assert_eq!(group.tenant_user_group().as_str(), "customers");
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(site.id().as_str(), "app");
        assert!(!site.upstream().is_empty());
        assert!(site.owner().is_empty());

        let host = Host::from_str("other.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(site.owner().as_str(), "team_a");
    }

    #[test]
    fn parse_site_limits() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    tcp_sock_speed_limit: 10MB
    request_rate_limit: 100
    request_max_alive: 32
    task_idle_max_count: 3
    resolve_strategy: ipv4first
    tcp_connect:
      max_retry: 2
      each_timeout: 5s
    tcp_remote_keepalive:
      enable: true
      idle_time: 60s
    tcp_remote_misc_opts:
      no_delay: true
    udp_remote_misc_opts:
      ttl: 64
    egress_path_id_map:
      default: path-a
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_ne!(site.tcp_sock_speed_limit, Default::default());
        assert!(site.request_rate_limit.is_some());
        assert_eq!(site.request_alive_max, Some(32));
        assert_eq!(site.task_idle_max_count, Some(3));
        assert!(site.resolve_strategy.is_some());
        assert_eq!(site.tcp_connect.unwrap().max_tries(), 3);
        assert!(site.tcp_remote_keepalive.is_enabled());
        assert_eq!(site.tcp_remote_misc_opts.unwrap().no_delay, Some(true));
        assert_eq!(site.udp_remote_misc_opts.unwrap().time_to_live, Some(64));
        assert_eq!(
            site.egress_path_id_map
                .get(&NodeName::from_str("default").unwrap())
                .map(String::as_str),
            Some("path-a")
        );
    }

    #[test]
    fn reject_unknown_site_field() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    dpi_protocol: http
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_missing_site_id() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - exact_match: app.internal
    upstream: 127.0.0.1:8080
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }
}
