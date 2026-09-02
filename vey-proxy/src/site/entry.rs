/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use crate::config::site::SiteConfig;

pub(crate) struct Site {
    config: Arc<SiteConfig>,
}

impl Site {
    pub(super) fn try_build(config: &Arc<SiteConfig>) -> anyhow::Result<Self> {
        Ok(Site {
            config: Arc::clone(config),
        })
    }

    pub(crate) fn config(&self) -> &Arc<SiteConfig> {
        &self.config
    }
}
