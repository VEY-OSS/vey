/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::Context;

use vey_types::metrics::NodeName;
use vey_types::route::HostMatch;

use super::Site;
use crate::config::site::SiteGroupConfig;

pub(crate) struct SiteGroup {
    config: Arc<SiteGroupConfig>,
    sites: HostMatch<Arc<Site>>,
}

impl SiteGroup {
    pub(super) fn new_no_config(name: &NodeName) -> Arc<Self> {
        let config = SiteGroupConfig::empty(name);
        Arc::new(SiteGroup {
            config: Arc::new(config),
            sites: HostMatch::default(),
        })
    }

    pub(super) fn new_with_config(config: SiteGroupConfig) -> anyhow::Result<Arc<Self>> {
        let sites = config
            .sites
            .try_build_arc(Site::try_build)
            .context("failed to build site group runtime")?;
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

    pub(crate) fn sites(&self) -> &HostMatch<Arc<Site>> {
        &self.sites
    }
}
