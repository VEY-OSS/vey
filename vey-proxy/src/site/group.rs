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
        let mut configs = Vec::new();
        config
            .sites
            .for_each_unique(|cfg| configs.push(Arc::clone(cfg)));

        let mut sites = AHashMap::with_capacity(configs.len());
        for cfg in configs {
            let id = cfg.id().clone();
            let site = Site::try_build(&cfg).context(format!("failed to build site {id}"))?;
            if sites.insert(id.clone(), Arc::new(site)).is_some() {
                return Err(anyhow!("duplicate site id {id}"));
            }
        }

        Ok(Arc::new(SiteGroup {
            config: Arc::new(config),
            sites,
        }))
    }

    pub(super) fn reload(&self, config: SiteGroupConfig) -> anyhow::Result<Arc<Self>> {
        Self::new_with_config(config)
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
