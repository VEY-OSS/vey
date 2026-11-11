/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use vey_types::metrics::NodeName;

use super::Site;

static REGISTRY: LazyLock<RwLock<HashMap<NodeName, Arc<Site>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub(super) fn get(id: &NodeName) -> Option<Arc<Site>> {
    REGISTRY.read().unwrap().get(id).cloned()
}

pub(super) fn insert(id: NodeName, site: Arc<Site>) {
    REGISTRY.write().unwrap().insert(id, site);
}

pub(super) fn replace(sites: HashMap<NodeName, Arc<Site>>) {
    *REGISTRY.write().unwrap() = sites;
}
