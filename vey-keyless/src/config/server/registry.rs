/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use foldhash::fast::FixedState;

use vey_types::metrics::NodeName;

use super::AnyKeyServerConfig;

static INITIAL_SERVER_CONFIG_REGISTRY: Mutex<
    HashMap<NodeName, Arc<AnyKeyServerConfig>, FixedState>,
> = Mutex::new(HashMap::with_hasher(FixedState::with_seed(0)));

pub(crate) fn clear() {
    let mut ht = INITIAL_SERVER_CONFIG_REGISTRY.lock().unwrap();
    ht.clear();
}

pub(super) fn add(server: AnyKeyServerConfig, replace: bool) -> anyhow::Result<()> {
    let name = server.name().clone();
    let server = Arc::new(server);
    let mut ht = INITIAL_SERVER_CONFIG_REGISTRY.lock().unwrap();
    if let Some(old) = ht.insert(name, server) {
        if replace {
            Ok(())
        } else {
            Err(anyhow!(
                "server with the same name {} is already existed",
                old.name()
            ))
        }
    } else {
        Ok(())
    }
}

pub(crate) fn get_all() -> Vec<Arc<AnyKeyServerConfig>> {
    let mut vec = Vec::new();
    let ht = INITIAL_SERVER_CONFIG_REGISTRY.lock().unwrap();
    for v in ht.values() {
        vec.push(Arc::clone(v));
    }
    vec
}
