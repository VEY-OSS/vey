/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use foldhash::fast::FixedState;

use vey_types::metrics::NodeName;

use super::SiteGroup;
use crate::config::site::SiteGroupConfig;

static RUNTIME_SITE_GROUP_REGISTRY: Mutex<HashMap<NodeName, Arc<SiteGroup>, FixedState>> =
    Mutex::new(HashMap::with_hasher(FixedState::with_seed(0)));

pub(super) fn add(name: NodeName, group: Arc<SiteGroup>) {
    let mut ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    ht.insert(name, group);
}

pub(super) fn foreach<F>(mut f: F)
where
    F: FnMut(&NodeName, &Arc<SiteGroup>),
{
    let ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    for (name, group) in ht.iter() {
        f(name, group);
    }
}

pub(super) fn get(name: &NodeName) -> Option<Arc<SiteGroup>> {
    let ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    ht.get(name).cloned()
}

pub(super) fn del(name: &NodeName) {
    let mut ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    ht.remove(name);
}

pub(crate) fn get_names() -> HashSet<NodeName> {
    let mut names = HashSet::new();
    let ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    for key in ht.keys() {
        names.insert(key.clone());
    }
    names
}

pub(super) fn get_config(name: &NodeName) -> Option<SiteGroupConfig> {
    let ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    ht.get(name).map(|g| g.clone_config())
}

pub(crate) fn get_or_insert_default(name: &NodeName) -> Arc<SiteGroup> {
    let mut ht = RUNTIME_SITE_GROUP_REGISTRY.lock().unwrap();
    ht.entry(name.clone())
        .or_insert_with(|| SiteGroup::new_no_config(name))
        .clone()
}
