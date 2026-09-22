/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use foldhash::fast::FixedState;

use vey_types::metrics::NodeName;

use super::SiteGroupConfig;

static INITIAL_SITE_GROUP_CONFIG_REGISTRY: Mutex<
    HashMap<NodeName, Arc<SiteGroupConfig>, FixedState>,
> = Mutex::new(HashMap::with_hasher(FixedState::with_seed(0)));

pub(crate) fn clear() {
    let mut ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    ht.clear();
}

pub(super) fn add(group: SiteGroupConfig) -> Option<SiteGroupConfig> {
    let name = group.name().clone();
    let group = Arc::new(group);
    let mut ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    ht.insert(name, group).map(|old| old.as_ref().clone())
}

pub(super) fn del(name: &NodeName) {
    let mut ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    ht.remove(name);
}

pub(super) fn get(name: &NodeName) -> Option<Arc<SiteGroupConfig>> {
    let ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    ht.get(name).cloned()
}

pub(super) fn get_all_names() -> Vec<NodeName> {
    let ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    ht.keys().cloned().collect()
}
