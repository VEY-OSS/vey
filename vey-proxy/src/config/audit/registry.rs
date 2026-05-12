/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use foldhash::fast::FixedState;

use vey_types::metrics::NodeName;

use super::AuditorConfig;

static INITIAL_AUDITOR_CONFIG_REGISTRY: Mutex<HashMap<NodeName, Arc<AuditorConfig>, FixedState>> =
    Mutex::new(HashMap::with_hasher(FixedState::with_seed(0)));

pub(crate) fn clear() {
    let mut ht = INITIAL_AUDITOR_CONFIG_REGISTRY.lock().unwrap();
    ht.clear();
}

pub(super) fn add(auditor: AuditorConfig, replace: bool) -> anyhow::Result<()> {
    let name = auditor.name().clone();
    let auditor = Arc::new(auditor);
    let mut ht = INITIAL_AUDITOR_CONFIG_REGISTRY
        .lock()
        .map_err(|e| anyhow!("failed to lock auditor config registry: {e}"))?;
    if let Some(old) = ht.insert(name, auditor) {
        if replace {
            Ok(())
        } else {
            Err(anyhow!(
                "auditor with the same name {} is already existed",
                old.name()
            ))
        }
    } else {
        Ok(())
    }
}

pub(crate) fn get_all() -> Vec<Arc<AuditorConfig>> {
    let mut vec = Vec::new();
    let ht = INITIAL_AUDITOR_CONFIG_REGISTRY.lock().unwrap();
    for v in ht.values() {
        vec.push(Arc::clone(v));
    }
    vec
}
