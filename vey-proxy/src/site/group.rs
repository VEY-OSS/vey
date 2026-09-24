/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use ahash::AHashMap;
use anyhow::{Context, anyhow};
use arc_swap::ArcSwapOption;
use log::debug;

use vey_types::metrics::NodeName;
use vey_types::route::HostMatch;

use super::Site;
use crate::auth::UserGroup;
use crate::config::site::SiteGroupConfig;

pub(crate) struct SiteGroup {
    config: Arc<SiteGroupConfig>,
    sites_by_id: AHashMap<NodeName, Arc<Site>>,
    sites_by_host: HostMatch<Arc<Site>>,
    tenant_user_group: Arc<ArcSwapOption<UserGroup>>,
}

impl SiteGroup {
    fn new(config: SiteGroupConfig) -> Self {
        let name = config.tenant_user_group();
        let tenant_user_group = Arc::new(ArcSwapOption::from(if name.is_empty() {
            None
        } else {
            Some(Arc::new(crate::auth::get_or_insert_default(name)))
        }));
        SiteGroup {
            config: Arc::new(config),
            sites_by_id: AHashMap::new(),
            sites_by_host: HostMatch::default(),
            tenant_user_group,
        }
    }

    pub(super) fn new_no_config(name: &NodeName) -> Arc<Self> {
        Arc::new(Self::new(SiteGroupConfig::empty(name)))
    }

    pub(super) fn new_with_config(config: SiteGroupConfig) -> anyhow::Result<Arc<Self>> {
        let mut group = Self::new(config);
        group.build_static_sites()?;
        group.import_external_sites()?;
        Ok(Arc::new(group))
    }

    pub(super) fn reload(&self, config: SiteGroupConfig) -> anyhow::Result<Arc<Self>> {
        let mut group = Self::new(config);
        group.reload_static_sites(self)?;
        group.import_external_sites()?;
        Ok(Arc::new(group))
    }

    fn build_static_sites(&mut self) -> anyhow::Result<()> {
        let config = Arc::clone(&self.config);
        let group_name = config.name();
        let tenant_user_group = Arc::clone(&self.tenant_user_group);
        let sites_by_host = config.sites.try_build_arc(|site_config| {
            let id = site_config.id();
            Site::try_build(group_name, site_config, Arc::clone(&tenant_user_group))
                .context(format!("failed to build site {id}"))
        })?;
        self.add_sites(sites_by_host);
        Ok(())
    }

    fn reload_static_sites(&mut self, old: &SiteGroup) -> anyhow::Result<()> {
        let config = Arc::clone(&self.config);
        let group_name = config.name();
        let tenant_user_group = Arc::clone(&self.tenant_user_group);
        let sites_by_host = config.sites.try_build_arc(|site_config| {
            let id = site_config.id();
            if let Some(prev) = old.sites_by_id.get(id)
                && prev.site_group() == group_name
            {
                prev.new_for_reload(site_config, Arc::clone(&tenant_user_group))
                    .context(format!("failed to reload site {id}"))
            } else {
                Site::try_build(group_name, site_config, Arc::clone(&tenant_user_group))
                    .context(format!("failed to build site {id}"))
            }
        })?;
        self.add_sites(sites_by_host);
        Ok(())
    }

    fn add_sites(&mut self, sites_by_host: HostMatch<Arc<Site>>) {
        sites_by_host.for_each_unique(|site| {
            self.sites_by_id.insert(site.id().clone(), Arc::clone(site));
        });
        self.sites_by_host = sites_by_host;
    }

    fn import_external_sites(&mut self) -> anyhow::Result<()> {
        let config = Arc::clone(&self.config);
        for import in config.imports() {
            let Some(source) = super::registry::get(import.site_group()) else {
                debug!(
                    "site group {} imports {}: source group is not loaded, skip",
                    config.name(),
                    import.site_group()
                );
                continue;
            };
            for site in source.iter_sites() {
                if !site.config().matches_any_tag(import.tags()) {
                    continue;
                }
                let id = site.id().clone();
                if self.sites_by_id.contains_key(&id) {
                    return Err(anyhow!(
                        "duplicate site id {id} imported from {}",
                        import.site_group()
                    ));
                }
                self.sites_by_host
                    .try_add_from_rules(site.config().host_match_rules(), Arc::clone(site))
                    .map_err(|e| anyhow!("host match conflict for site {id}: {e}"))?;
                self.sites_by_id.insert(id, Arc::clone(site));
            }
        }
        Ok(())
    }

    pub(super) fn clone_config(&self) -> SiteGroupConfig {
        self.config.as_ref().clone()
    }

    pub(super) fn _depend_on_site_group(&self, name: &NodeName) -> bool {
        self.config
            .imports()
            .iter()
            .any(|import| import.site_group().eq(name))
    }

    pub(crate) fn sites_by_host(&self) -> &HostMatch<Arc<Site>> {
        &self.sites_by_host
    }

    pub(crate) fn site(&self, id: &NodeName) -> Option<Arc<Site>> {
        self.sites_by_id.get(id).cloned()
    }

    pub(super) fn iter_sites(&self) -> impl Iterator<Item = &Arc<Site>> {
        self.sites_by_id.values()
    }

    pub(super) fn update_tenant_user_group_in_place(&self, user_group: &NodeName) -> bool {
        let group = if user_group.is_empty() {
            None
        } else {
            Some(Arc::new(crate::auth::get_or_insert_default(user_group)))
        };
        let mut updated = false;
        if self.config.tenant_user_group().eq(user_group) {
            self.tenant_user_group.store(group.clone());
            updated = true;
        }
        for site in self.sites_by_id.values() {
            if site.refresh_tenant_user_group(user_group, group.clone()) {
                updated = true;
            }
        }
        updated
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use yaml_rust::YamlLoader;

    use super::*;

    fn parse_group(s: &str) -> SiteGroupConfig {
        let yaml = YamlLoader::load_from_str(s).unwrap();
        let yaml_rust::Yaml::Hash(map) = &yaml[0] else {
            panic!("expected map");
        };
        SiteGroupConfig::parse(map, None).unwrap()
    }

    #[test]
    fn reload_reuses_site_stats() {
        let config = parse_group(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    request_rate_limit: 100
    request_max_alive: 32
"#,
        );
        let group = SiteGroup::new_with_config(config.clone()).unwrap();
        let id = NodeName::from_str("app").unwrap();
        let site = group.sites_by_id.get(&id).unwrap();
        let stats = Arc::clone(site.stats());

        let reloaded = group.reload(config).unwrap();
        let site2 = reloaded.sites_by_id.get(&id).unwrap();
        assert!(Arc::ptr_eq(&stats, site2.stats()));
        assert!(site.http1_pool().is_none());
        assert!(site2.http1_pool().is_none());
        let h2_ptr = site.http2_pool() as *const _;
        assert_eq!(h2_ptr, site2.http2_pool() as *const _);
    }

    #[test]
    fn reload_reuses_http1_pool_when_origin_unchanged() {
        let config = parse_group(
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
        );
        let group = SiteGroup::new_with_config(config.clone()).unwrap();
        let id = NodeName::from_str("app").unwrap();
        let site = group.sites_by_id.get(&id).unwrap();
        let pool = site.http1_pool().expect("http1 pool");
        let pool_ptr = pool as *const _;

        let reloaded = group.reload(config).unwrap();
        let site2 = reloaded.sites_by_id.get(&id).unwrap();
        let pool2 = site2.http1_pool().expect("http1 pool after reload");
        assert_eq!(pool_ptr, pool2 as *const _);
    }

    #[test]
    fn reload_drops_http1_pool_when_disabled() {
        let config = parse_group(
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
        );
        let group = SiteGroup::new_with_config(config).unwrap();
        let id = NodeName::from_str("app").unwrap();
        assert!(group.sites_by_id.get(&id).unwrap().http1_pool().is_some());

        let disabled = parse_group(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
"#,
        );
        let reloaded = group.reload(disabled).unwrap();
        assert!(
            reloaded
                .sites_by_id
                .get(&id)
                .unwrap()
                .http1_pool()
                .is_none()
        );
    }

    #[test]
    fn reload_reuses_http2_pool_when_origin_unchanged() {
        let config = parse_group(
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
        );
        let group = SiteGroup::new_with_config(config.clone()).unwrap();
        let id = NodeName::from_str("app").unwrap();
        let site = group.sites_by_id.get(&id).unwrap();
        let pool_ptr = site.http2_pool() as *const _;

        let reloaded = group.reload(config).unwrap();
        let site2 = reloaded.sites_by_id.get(&id).unwrap();
        assert_eq!(pool_ptr, site2.http2_pool() as *const _);
    }

    #[test]
    fn reload_rebuilds_http2_pool_when_pool_config_changes() {
        let config = parse_group(
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
        );
        let group = SiteGroup::new_with_config(config).unwrap();
        let id = NodeName::from_str("app").unwrap();
        let pool_ptr = group.sites_by_id.get(&id).unwrap().http2_pool() as *const _;

        let changed = parse_group(
            r#"
name: local
static_sites:
  - id: app
    exact_match: app.internal
    upstream: 127.0.0.1:8080
    http:
      h2:
        connection_pool:
          max_idle_count: 8
          idle_timeout: 30s
"#,
        );
        let reloaded = group.reload(changed).unwrap();
        assert_ne!(
            pool_ptr,
            reloaded.sites_by_id.get(&id).unwrap().http2_pool() as *const _
        );
    }

    #[test]
    fn import_sites_by_tag() {
        let source_name = NodeName::from_str("ut_import_src").unwrap();
        let source = SiteGroup::new_with_config(parse_group(
            r#"
name: ut_import_src
tenant_user_group: customers
static_sites:
  - id: public_app
    tags:
      - public
      - cdn
    exact_match: public.example.com
    upstream: 127.0.0.1:8080
    owner: team_a
  - id: private_app
    tags: private
    exact_match: private.example.com
    upstream: 127.0.0.1:8081
"#,
        ))
        .unwrap();
        super::super::registry::add(source_name.clone(), Arc::clone(&source));

        let importer = SiteGroup::new_with_config(parse_group(
            r#"
name: ut_import_dst
tenant_user_group: edge_users
import:
  - site_group: ut_import_src
    tags:
      - public
      - missing
static_sites:
  - id: local
    exact_match: local.example.com
    upstream: 127.0.0.1:9000
"#,
        ));
        super::super::registry::del(&source_name);
        let importer = importer.unwrap();

        let public_id = NodeName::from_str("public_app").unwrap();
        let private_id = NodeName::from_str("private_app").unwrap();
        let local_id = NodeName::from_str("local").unwrap();
        assert!(importer.sites_by_id.get(&public_id).is_some());
        assert!(importer.sites_by_id.get(&private_id).is_none());
        assert!(importer.sites_by_id.get(&local_id).is_some());

        let imported = importer.sites_by_id.get(&public_id).unwrap();
        assert!(Arc::ptr_eq(
            imported,
            source.sites_by_id.get(&public_id).unwrap()
        ));
        assert_eq!(
            imported.tenant_user_group().unwrap().name().as_str(),
            "customers"
        );
        assert_eq!(imported.owner().as_str(), "team_a");
        assert_eq!(
            importer
                .sites_by_id
                .get(&local_id)
                .unwrap()
                .tenant_user_group()
                .unwrap()
                .name()
                .as_str(),
            "edge_users"
        );

        let host = vey_types::net::Host::from_str("public.example.com").unwrap();
        assert_eq!(
            importer.sites_by_host().get(&host).unwrap().id(),
            &public_id
        );
        let local_host = vey_types::net::Host::from_str("local.example.com").unwrap();
        assert_eq!(
            importer.sites_by_host().get(&local_host).unwrap().id(),
            &local_id
        );
        let private_host = vey_types::net::Host::from_str("private.example.com").unwrap();
        assert!(importer.sites_by_host().get(&private_host).is_none());
    }

    #[test]
    fn import_rejects_duplicate_site_id() {
        let source_name = NodeName::from_str("ut_import_dup_src").unwrap();
        let source = SiteGroup::new_with_config(parse_group(
            r#"
name: ut_import_dup_src
static_sites:
  - id: app
    tags: public
    exact_match: shared.example.com
    upstream: 127.0.0.1:8080
"#,
        ))
        .unwrap();
        super::super::registry::add(source_name.clone(), Arc::clone(&source));

        let importer = SiteGroup::new_with_config(parse_group(
            r#"
name: ut_import_dup_dst
import:
  - site_group: ut_import_dup_src
    tags: public
static_sites:
  - id: app
    exact_match: local.example.com
    upstream: 127.0.0.1:9000
"#,
        ));
        super::super::registry::del(&source_name);
        assert!(importer.is_err());
    }

    #[test]
    fn reload_picks_up_imported_site_changes() {
        let source_name = NodeName::from_str("ut_import_reload_src").unwrap();
        let source = SiteGroup::new_with_config(parse_group(
            r#"
name: ut_import_reload_src
static_sites:
  - id: public_app
    tags: public
    exact_match: public.example.com
    upstream: 127.0.0.1:8080
"#,
        ))
        .unwrap();
        super::super::registry::add(source_name.clone(), Arc::clone(&source));

        let importer_config = parse_group(
            r#"
name: ut_import_reload_dst
import:
  - site_group: ut_import_reload_src
    tags: public
"#,
        );
        let importer = SiteGroup::new_with_config(importer_config.clone()).unwrap();
        let public_id = NodeName::from_str("public_app").unwrap();
        assert!(importer.sites_by_id.get(&public_id).is_some());

        let source2 = SiteGroup::new_with_config(parse_group(
            r#"
name: ut_import_reload_src
static_sites:
  - id: public_app
    tags: public
    exact_match: public.example.com
    upstream: 10.0.0.1:8080
  - id: extra
    tags: public
    exact_match: extra.example.com
    upstream: 10.0.0.2:8080
"#,
        ))
        .unwrap();
        let source_site = Arc::clone(source2.sites_by_id.get(&public_id).unwrap());
        super::super::registry::add(source_name.clone(), source2);

        let reloaded = importer.reload(importer_config).unwrap();
        super::super::registry::del(&source_name);
        assert!(Arc::ptr_eq(
            reloaded.sites_by_id.get(&public_id).unwrap(),
            &source_site
        ));

        let site = reloaded.sites_by_id.get(&public_id).unwrap();
        let upstream = site
            .select_upstream(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST))
            .unwrap();
        assert_eq!(upstream.port(), 8080);
        assert_eq!(upstream.host().to_string(), "10.0.0.1");
        assert!(
            reloaded
                .sites_by_id
                .get(&NodeName::from_str("extra").unwrap())
                .is_some()
        );
    }
}
