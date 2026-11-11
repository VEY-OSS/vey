/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_types::metrics::NodeName;
use vey_types::net::TlsCertUsage;

use crate::config::site::{SiteConfig, SiteKeyCert};

mod registry;

mod site;
pub(crate) use site::Site;

pub fn load_all() {
    let sites = crate::config::site::get_all()
        .iter()
        .map(|config| (config.id().clone(), Arc::new(Site::from_config(config))))
        .collect();
    registry::replace(sites);
}

pub(crate) fn load_one(config: &SiteConfig) {
    let id = config.id().clone();
    let site = Arc::new(Site::from_config(config));
    registry::insert(id, site);
}

pub(crate) fn get(id: &str, usage: TlsCertUsage) -> Option<Arc<SiteKeyCert>> {
    let id = id.parse::<NodeName>().ok()?;
    registry::get(&id)?.cert(usage)
}
