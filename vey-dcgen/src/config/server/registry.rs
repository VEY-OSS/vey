/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeMap;
use std::sync::Mutex;

use vey_types::metrics::NodeName;

use super::AnyServerConfig;

static SERVER_CONFIG_REGISTRY: Mutex<BTreeMap<NodeName, AnyServerConfig>> =
    Mutex::new(BTreeMap::new());

pub(super) fn add(server: AnyServerConfig) -> Option<AnyServerConfig> {
    let name = server.name().clone();
    let mut registry = SERVER_CONFIG_REGISTRY.lock().unwrap();
    registry.insert(name, server)
}

pub(crate) fn clear() {
    SERVER_CONFIG_REGISTRY.lock().unwrap().clear();
}

pub(crate) fn is_empty() -> bool {
    SERVER_CONFIG_REGISTRY.lock().unwrap().is_empty()
}

pub(crate) fn get_all() -> Vec<AnyServerConfig> {
    SERVER_CONFIG_REGISTRY
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect()
}
