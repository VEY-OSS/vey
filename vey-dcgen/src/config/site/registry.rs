/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeMap;
use std::sync::Mutex;

use vey_types::metrics::NodeName;

use super::SiteConfig;

static SITE_CONFIG_REGISTRY: Mutex<BTreeMap<NodeName, SiteConfig>> = Mutex::new(BTreeMap::new());

pub(super) fn add(config: SiteConfig) -> Option<SiteConfig> {
    let id = config.id().clone();
    let mut registry = SITE_CONFIG_REGISTRY.lock().unwrap();
    registry.insert(id, config)
}

pub(super) fn get(id: &NodeName) -> Option<SiteConfig> {
    SITE_CONFIG_REGISTRY.lock().unwrap().get(id).cloned()
}

pub(crate) fn clear() {
    SITE_CONFIG_REGISTRY.lock().unwrap().clear();
}

pub(crate) fn get_all() -> Vec<SiteConfig> {
    SITE_CONFIG_REGISTRY
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect()
}
