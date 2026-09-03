/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use ahash::AHashMap;
use anyhow::{Context, anyhow};

use vey_types::metrics::NodeName;

use super::Site;
use crate::config::site::SiteGroupConfig;

pub(crate) struct SiteGroup {
    config: Arc<SiteGroupConfig>,
    sites: AHashMap<NodeName, Arc<Site>>,
}

impl SiteGroup {
    pub(super) fn new_no_config(name: &NodeName) -> Arc<Self> {
        let config = SiteGroupConfig::empty(name);
        Arc::new(SiteGroup {
            config: Arc::new(config),
            sites: AHashMap::new(),
        })
    }

    pub(super) fn new_with_config(config: SiteGroupConfig) -> anyhow::Result<Arc<Self>> {
        Self::build(config, None)
    }

    pub(super) fn reload(&self, config: SiteGroupConfig) -> anyhow::Result<Arc<Self>> {
        Self::build(config, Some(&self.sites))
    }

    fn build(
        config: SiteGroupConfig,
        old_sites: Option<&AHashMap<NodeName, Arc<Site>>>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut configs = Vec::new();
        config
            .sites
            .for_each_unique(|cfg| configs.push(Arc::clone(cfg)));

        let group_name = config.name().clone();
        let mut sites = AHashMap::with_capacity(configs.len());
        for cfg in configs {
            let id = cfg.id().clone();
            let site = if let Some(old) = old_sites.and_then(|m| m.get(&id)) {
                old.new_for_reload(&cfg)
                    .context(format!("failed to reload site {id}"))?
            } else {
                Site::try_build(&group_name, &cfg).context(format!("failed to build site {id}"))?
            };
            let site = Arc::new(site);
            if sites.insert(site.id().clone(), site).is_some() {
                return Err(anyhow!("duplicate site id {id}"));
            }
        }

        Ok(Arc::new(SiteGroup {
            config: Arc::new(config),
            sites,
        }))
    }

    pub(super) fn clone_config(&self) -> SiteGroupConfig {
        self.config.as_ref().clone()
    }

    pub(crate) fn config(&self) -> &SiteGroupConfig {
        self.config.as_ref()
    }

    pub(crate) fn get_site(&self, id: &NodeName) -> Option<Arc<Site>> {
        self.sites.get(id).cloned()
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
        let site = group.get_site(&id).unwrap();
        let stats = Arc::clone(site.stats());

        let reloaded = group.reload(config).unwrap();
        let site2 = reloaded.get_site(&id).unwrap();
        assert!(Arc::ptr_eq(&stats, site2.stats()));
    }
}
