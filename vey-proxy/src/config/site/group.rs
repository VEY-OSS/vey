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
    pub(crate) sites: HostMatch<Arc<SiteConfig>>,
}

impl SiteGroupConfig {
    pub(crate) fn name(&self) -> &NodeName {
        &self.name
    }

    pub(crate) fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    pub(crate) fn empty(name: &NodeName) -> Self {
        SiteGroupConfig {
            name: name.clone(),
            position: None,
            sites: HostMatch::default(),
        }
    }

    fn new(position: Option<YamlDocPosition>) -> Self {
        SiteGroupConfig {
            name: NodeName::default(),
            position,
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
    use vey_types::net::Host;
    use yaml_rust::YamlLoader;

    #[test]
    fn parse_static_sites() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        assert_eq!(group.name().as_str(), "local");
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(site.id().as_str(), "app");
        assert!(!site.upstream().is_empty());
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
