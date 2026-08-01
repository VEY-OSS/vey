/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{Context, anyhow};
use foldhash::fast::FixedState;

use vey_types::metrics::NodeName;

use super::{ArcKeyServer, ArcKeyServerInternal};
use crate::config::server::AnyKeyServerConfig;

static RUNTIME_SERVER_REGISTRY: Mutex<ServerRegistry> = Mutex::new(ServerRegistry::new());
static OFFLINE_SERVER_SET: Mutex<Vec<ArcKeyServerInternal>> = Mutex::new(Vec::new());

struct ServerRegistry {
    inner: HashMap<NodeName, ArcKeyServerInternal, FixedState>,
}

impl ServerRegistry {
    const fn new() -> Self {
        ServerRegistry {
            inner: HashMap::with_hasher(FixedState::with_seed(0)),
        }
    }

    fn add(&mut self, name: NodeName, server: ArcKeyServerInternal) -> anyhow::Result<()> {
        server._start_runtime(server.clone())?;
        if let Some(old_server) = self.inner.insert(name, server) {
            old_server._abort_runtime();
            add_offline(old_server);
        }
        Ok(())
    }

    fn del(&mut self, name: &NodeName) {
        if let Some(old_server) = self.inner.remove(name) {
            old_server._abort_runtime();
            add_offline(old_server);
        }
    }
}

pub(super) fn add_offline(old_server: ArcKeyServerInternal) {
    let mut set = OFFLINE_SERVER_SET.lock().unwrap();
    set.push(old_server);
}

pub(super) fn retain_offline() {
    let mut set = OFFLINE_SERVER_SET.lock().unwrap();
    set.retain(|server| {
        if server.alive_count() == 0 {
            Arc::strong_count(server) > 1
        } else {
            let quit_policy = server.quit_policy().clone();
            if !quit_policy.force_quit_scheduled() {
                quit_policy.set_force_quit_scheduled();
                tokio::spawn(async move {
                    let wait_time = vey_daemon::runtime::config::get_task_wait_timeout();
                    tokio::time::sleep(wait_time).await;
                    quit_policy.set_force_quit();
                });
            }
            true
        }
    });
}

pub(super) fn foreach_offline<F>(mut f: F)
where
    F: FnMut(&ArcKeyServerInternal),
{
    let set = OFFLINE_SERVER_SET.lock().unwrap();
    for server in set.iter() {
        f(server)
    }
}

pub(super) fn add(name: NodeName, server: ArcKeyServerInternal) -> anyhow::Result<()> {
    let mut sr = RUNTIME_SERVER_REGISTRY
        .lock()
        .map_err(|e| anyhow!("failed to lock server registry: {e}"))?;
    sr.add(name, server)
}

pub(super) fn add_lazy(name: NodeName, server: ArcKeyServerInternal) {
    let mut sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    // no runtime started, and the replaced one is not moved to offline
    sr.inner.insert(name, server);
}

pub(super) fn del(name: &NodeName) {
    let mut sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.del(name);
}

pub(crate) fn get_names() -> HashSet<NodeName> {
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.inner.keys().cloned().collect()
}

pub(super) fn get_config(name: &NodeName) -> Option<AnyKeyServerConfig> {
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.inner.get(name).map(|server| server._clone_config())
}

pub(crate) fn get_server(name: &NodeName) -> Option<ArcKeyServer> {
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.inner.get(name).map(|server| {
        let server: ArcKeyServer = server.clone();
        server
    })
}

pub(super) fn reload_and_respawn(name: &NodeName, config: AnyKeyServerConfig) -> anyhow::Result<()> {
    let mut sr = RUNTIME_SERVER_REGISTRY
        .lock()
        .map_err(|e| anyhow!("failed to lock server registry: {e}"))?;
    let Some(old_server) = sr.inner.get(name).cloned() else {
        return Err(anyhow!("no server with name {name} found"));
    };

    let server = old_server._reload(config)?;
    sr.add(name.clone(), server)
}

pub(crate) fn foreach_online<F>(mut f: F)
where
    F: FnMut(&NodeName, &ArcKeyServer),
{
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    for (name, server) in sr.inner.iter() {
        let server: ArcKeyServer = server.clone();
        f(name, &server)
    }
}

pub(super) fn foreach_online_internal<F>(mut f: F)
where
    F: FnMut(&NodeName, &ArcKeyServerInternal),
{
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    for (name, server) in sr.inner.iter() {
        f(name, server)
    }
}

pub(super) fn foreach_start_runtime() -> anyhow::Result<()> {
    let sr = RUNTIME_SERVER_REGISTRY
        .lock()
        .map_err(|e| anyhow!("failed to lock server registry: {e}"))?;
    for (name, server) in sr.inner.iter() {
        server
            ._start_runtime(server.clone())
            .context(format!("failed to start runtime for {name}"))?;
    }
    Ok(())
}

pub(super) fn move_all_offline() {
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    for server in sr.inner.values() {
        server._abort_runtime();
        add_offline(Arc::clone(server));
    }
}
