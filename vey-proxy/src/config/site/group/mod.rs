/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;
use vey_types::route::HostMatch;
use vey_yaml::YamlDocPosition;

use super::SiteConfig;

mod import;
use import::SiteGroupImportConfig;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SiteGroupConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    tenant_user_group: NodeName,
    imports: Vec<SiteGroupImportConfig>,
    pub(crate) sites: HostMatch<Arc<SiteConfig>>,
}

impl SiteGroupConfig {
    pub(crate) fn name(&self) -> &NodeName {
        &self.name
    }

    pub(crate) fn tenant_user_group(&self) -> &NodeName {
        &self.tenant_user_group
    }

    pub(crate) fn imports(&self) -> &[SiteGroupImportConfig] {
        &self.imports
    }

    pub(crate) fn dependent_site_group(&self) -> Option<BTreeSet<NodeName>> {
        if self.imports.is_empty() {
            return None;
        }
        let mut set = BTreeSet::new();
        for import in &self.imports {
            set.insert(import.site_group().clone());
        }
        Some(set)
    }

    pub(crate) fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    pub(crate) fn empty(name: &NodeName) -> Self {
        SiteGroupConfig {
            name: name.clone(),
            position: None,
            tenant_user_group: NodeName::default(),
            imports: Vec::new(),
            sites: HostMatch::default(),
        }
    }

    fn new(position: Option<YamlDocPosition>) -> Self {
        SiteGroupConfig {
            name: NodeName::default(),
            position,
            tenant_user_group: NodeName::default(),
            imports: Vec::new(),
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
            "import" | "imports" => {
                self.imports = vey_yaml::value::as_list(v, SiteGroupImportConfig::parse)
                    .context(format!("invalid site group import list for key {k}"))?;
                Ok(())
            }
            "static_sites" | "sites" => {
                self.sites = vey_yaml::value::as_host_matched_obj_with(
                    v,
                    self.position.as_ref(),
                    SiteConfig::save_host_rules,
                )
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
        let mut seen = BTreeSet::new();
        for import in &self.imports {
            if import.site_group().eq(&self.name) {
                return Err(anyhow!("site group {} cannot import itself", self.name));
            }
            if !seen.insert(import.site_group().clone()) {
                return Err(anyhow!(
                    "duplicate import of site group {}",
                    import.site_group()
                ));
            }
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
        assert!(site.tags.is_empty());
    }

    #[test]
    fn parse_multi_ip_upstream() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream_pick_policy: ketama
    upstream:
      - 10.0.0.1:8080
      - addr: 10.0.0.2:8080
        weight: 2
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert!(site.upstream().single().is_none());
        assert_eq!(site.upstream().peers().len(), 2);
        assert_eq!(
            site.upstream().pick_policy(),
            vey_types::collection::SelectivePickPolicy::Ketama
        );
        assert!(site.tls_name.is_empty());
    }

    #[test]
    fn reject_domain_and_duplicate_upstream() {
        let domain = r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream:
      - example.com:80
"#;
        let yaml = YamlLoader::load_from_str(domain).unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let err = SiteGroupConfig::parse(map, None).unwrap_err();
        assert!(format!("{err:#}").contains("ip:port"), "{err:#}");

        let duplicate = r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream:
      - 10.0.0.1:8080
      - 10.0.0.1:8080
"#;
        let yaml = YamlLoader::load_from_str(duplicate).unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let err = SiteGroupConfig::parse(map, None).unwrap_err();
        assert!(format!("{err:#}").contains("duplicate"), "{err:#}");
    }

    #[test]
    fn parse_site_tags() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    tags:
      - public
      - edge
    exact_match: app.internal
    upstream: 127.0.0.1:8080
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert!(site.tags.contains(&NodeName::from_str("public").unwrap()));
        assert!(site.tags.contains(&NodeName::from_str("edge").unwrap()));
        assert!(
            site.matches_any_tag(
                &[NodeName::from_str("public").unwrap()]
                    .into_iter()
                    .collect()
            )
        );
        assert!(
            !site.matches_any_tag(
                &[NodeName::from_str("private").unwrap()]
                    .into_iter()
                    .collect()
            )
        );
    }

    #[test]
    fn parse_import() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: edge
import:
  - site_group: shared
    tags:
      - public
      - cdn
  - name: extra
    tag: edge
static_sites:
  - id: local
    exact_match: local.internal
    upstream: 127.0.0.1:9000
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        assert_eq!(group.imports().len(), 2);
        assert_eq!(group.imports()[0].site_group().as_str(), "shared");
        assert_eq!(group.imports()[0].tags().len(), 2);
        assert!(
            group.imports()[0]
                .tags()
                .contains(&NodeName::from_str("public").unwrap())
        );
        assert_eq!(group.imports()[1].site_group().as_str(), "extra");
        assert!(
            group.imports()[1]
                .tags()
                .contains(&NodeName::from_str("edge").unwrap())
        );
        let deps = group.dependent_site_group().unwrap();
        assert!(deps.contains(&NodeName::from_str("shared").unwrap()));
        assert!(deps.contains(&NodeName::from_str("extra").unwrap()));
        assert!(!deps.contains(&NodeName::from_str("missing").unwrap()));
    }

    #[test]
    fn reject_self_import() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
import:
  - site_group: local
    tags: public
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_duplicate_import() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
import:
  - site_group: shared
    tags: public
  - site_group: shared
    tags: edge
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_import_without_tags() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
import:
  - site_group: shared
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn site_covers_host_from_own_match_rules() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: exact-http
    exact_match: a.example.com
    upstream: 127.0.0.1:8080
  - id: suffix-https
    suffix_match: example.com
    upstream: 127.0.0.1:8081
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();

        let exact_host = Host::from_str("a.example.com").unwrap();
        let sni_host = Host::from_str("www.example.com").unwrap();
        let sibling = Host::from_str("b.example.com").unwrap();
        let other = Host::from_str("other.com").unwrap();

        let exact = group.sites.get_matched(&exact_host).unwrap();
        assert_eq!(exact.id().as_str(), "exact-http");
        assert!(exact.covers_host(&exact_host));
        assert!(!exact.covers_host(&sni_host));

        let suffix = group.sites.get_matched(&sni_host).unwrap();
        assert_eq!(suffix.id().as_str(), "suffix-https");
        assert!(suffix.covers_host(&sni_host));
        assert!(suffix.covers_host(&sibling));
        assert!(suffix.covers_host(&exact_host));
        assert!(!suffix.covers_host(&other));
    }

    #[test]
    fn parse_dpi_protocol() {
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
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(site.dpi_protocol, Some(vey_dpi::MaybeProtocol::Http));
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
    fn parse_site_http() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      rsp_header_recv_timeout: 8s
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(
            site.http.rsp_hdr_recv_timeout,
            Some(std::time::Duration::from_secs(8))
        );
        assert_eq!(site.http.h1.connection_pool, None);
        assert!(site.http.h1.upstream_keepalive.is_enabled());
        assert_eq!(
            site.http.h1.upstream_keepalive.idle_expire(),
            std::time::Duration::from_secs(60)
        );
        assert_eq!(
            site.http.h2.connection_pool,
            vey_types::net::ConnectionPoolConfig::default()
        );
        assert!(site.http.forwarded_trusted_from.is_empty());
        assert_eq!(
            site.http.forwarded_header_type,
            vey_types::net::HttpForwardedHeaderType::Classic
        );
    }

    #[test]
    fn parse_site_http_forwarded_header_type() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      forwarded_header_type: standard
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(
            site.http.forwarded_header_type,
            vey_types::net::HttpForwardedHeaderType::Standard
        );
    }

    #[test]
    fn parse_site_http_forwarded_trusted_from() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      forwarded_trusted_from:
        - 192.168.1.1
        - 10.0.0.0/8
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(
            site.http.forwarded_trusted_from,
            vec![
                ip_network::IpNetwork::from_str("10.0.0.0/8").unwrap(),
                ip_network::IpNetwork::from_str("192.168.1.1/32").unwrap(),
            ]
        );
    }

    #[test]
    fn parse_site_http_h1_connection_pool_empty() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h1:
        connection_pool: {}
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(
            site.http.h1.connection_pool,
            Some(vey_types::net::ConnectionPoolConfig::default())
        );
    }

    #[test]
    fn parse_site_http_h1_connection_pool_map() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h1:
        connection_pool:
          max_idle_count: 16
          idle_timeout: 30s
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        let pool = site.http.h1.connection_pool.expect("h1 connection_pool");
        assert_eq!(pool.max_idle_count(), 16);
        assert_eq!(pool.idle_timeout(), std::time::Duration::from_secs(30));
    }

    #[test]
    fn parse_site_http_h1_upstream_keepalive() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h1:
        upstream_keepalive:
          enable: false
          idle_expire: 15s
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert!(!site.http.h1.upstream_keepalive.is_enabled());
        assert_eq!(
            site.http.h1.upstream_keepalive.idle_expire(),
            std::time::Duration::ZERO
        );
    }

    #[test]
    fn parse_site_http_h1_nested_connection_pool() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h1:
        connection_pool:
          max_idle_count: 8
      h2:
        connection_pool:
          max_idle_count: 4
          idle_timeout: 20s
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        let h1_pool = site.http.h1.connection_pool.expect("h1 connection_pool");
        assert_eq!(h1_pool.max_idle_count(), 8);
        assert_eq!(site.http.h2.connection_pool.max_idle_count(), 4);
        assert_eq!(
            site.http.h2.connection_pool.idle_timeout(),
            std::time::Duration::from_secs(20)
        );
    }

    #[test]
    fn parse_site_http_h2_connection_pool_empty() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h2:
        connection_pool: {}
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(
            site.http.h2.connection_pool,
            vey_types::net::ConnectionPoolConfig::default()
        );
        assert_eq!(site.http.h1.connection_pool, None);
    }

    #[test]
    fn parse_site_http_h2_connection_pool_map() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h2:
        connection_pool:
          max_idle_count: 16
          idle_timeout: 30s
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        let pool = site.http.h2.connection_pool;
        assert_eq!(pool.max_idle_count(), 16);
        assert_eq!(pool.idle_timeout(), std::time::Duration::from_secs(30));
    }

    #[test]
    fn reject_invalid_site_http_h2_connection_pool() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h2:
        connection_pool: sticky
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_unknown_site_http_h2_field() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h2:
        bogus_field: 1
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_invalid_site_http_h1_connection_pool() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h1:
        connection_pool: sticky
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn reject_legacy_site_http_h1_connection_pool_key() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h1_connection_pool: {}
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
    }

    #[test]
    fn parse_empty_site_http() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http: {}
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        let group = SiteGroupConfig::parse(map, None).unwrap();
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(site.http, Default::default());
    }

    #[test]
    fn omit_site_http_uses_default() {
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
        let host = Host::from_str("app.internal").unwrap();
        let site = group.sites.get(&host).unwrap();
        assert_eq!(site.http, Default::default());
    }

    #[test]
    fn reject_unknown_site_http_field() {
        let yaml = YamlLoader::load_from_str(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      bogus_field: 1
"#,
        )
        .unwrap();
        let Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        assert!(SiteGroupConfig::parse(map, None).is_err());
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
    bogus_field: 1
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
