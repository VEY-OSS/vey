/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
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

pub(super) fn add(group: SiteGroupConfig, replace: bool) -> anyhow::Result<()> {
    let name = group.name().clone();
    let group = Arc::new(group);
    let mut ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    if let Some(old) = ht.insert(name, group) {
        if replace {
            Ok(())
        } else {
            Err(anyhow!(
                "site group with the same name {} is already existed",
                old.name()
            ))
        }
    } else {
        Ok(())
    }
}

pub(crate) fn get_all() -> Vec<Arc<SiteGroupConfig>> {
    let mut vec = Vec::new();
    let ht = INITIAL_SITE_GROUP_CONFIG_REGISTRY.lock().unwrap();
    for v in ht.values() {
        vec.push(Arc::clone(v));
    }
    vec
}
