/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ops::Deref;
use std::sync::Arc;

use super::super::CommonTaskContext;
use crate::auth::UserTrafficStats;
use crate::config::server::ServerConfig;
use crate::serve::ServerStats;
use crate::site::SiteContext;

#[derive(Clone)]
pub(crate) struct H1TaskContext {
    pub(crate) common: CommonTaskContext,
    pub(crate) site_ctx: Option<SiteContext>,
}

impl H1TaskContext {
    pub(crate) fn site_io(&self, site_ctx: &SiteContext) -> Arc<UserTrafficStats> {
        site_ctx.fetch_traffic_stats(
            self.server_config.name(),
            self.server_stats.share_extra_tags(),
        )
    }

    pub(crate) fn pinned_site_io(&self) -> Option<Arc<UserTrafficStats>> {
        self.site_ctx
            .as_ref()
            .map(|site_ctx| self.site_io(site_ctx))
    }
}

impl Deref for H1TaskContext {
    type Target = CommonTaskContext;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}
