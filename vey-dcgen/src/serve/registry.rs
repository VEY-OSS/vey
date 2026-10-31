/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use foldhash::fast::FixedState;

use vey_types::metrics::NodeName;

use super::{ArcServer, ArcServerInternal};
use crate::config::server::AnyServerConfig;

static RUNTIME_SERVER_REGISTRY: Mutex<ServerRegistry> = Mutex::new(ServerRegistry::new());
static OFFLINE_SERVER_SET: Mutex<Vec<ArcServerInternal>> = Mutex::new(Vec::new());

struct ServerRegistry {
    inner: HashMap<NodeName, ArcServerInternal, FixedState>,
}

impl ServerRegistry {
    const fn new() -> Self {
        ServerRegistry {
            inner: HashMap::with_hasher(FixedState::with_seed(0)),
        }
    }

    fn add(&mut self, name: NodeName, server: ArcServerInternal) -> anyhow::Result<()> {
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

    fn get_or_insert_default(&mut self, name: &NodeName) -> ArcServer {
        let server = self
            .inner
            .entry(name.clone())
            .or_insert_with(|| super::dummy::DummyServer::prepare_default(name));
        let server: ArcServer = server.clone();
        server
    }

    fn reload_and_respawn(
        &mut self,
        name: &NodeName,
        config: AnyServerConfig,
    ) -> anyhow::Result<()> {
        let Some(old_server) = self.inner.get(name).cloned() else {
            return Err(anyhow!("no server with name {name} found"));
        };
        let server = old_server._reload_with_new_notifier(config)?;
        self.add(name.clone(), server)
    }
}

pub(super) fn add_offline(old_server: ArcServerInternal) {
    let mut set = OFFLINE_SERVER_SET.lock().unwrap();
    set.push(old_server);
}

pub(super) fn retain_offline() {
    let mut set = OFFLINE_SERVER_SET.lock().unwrap();
    set.retain(|server| server.alive_count() > 0 || Arc::strong_count(server) > 1);
}

pub(super) fn foreach_offline<F>(mut f: F)
where
    F: FnMut(&ArcServerInternal),
{
    let set = OFFLINE_SERVER_SET.lock().unwrap();
    for server in set.iter() {
        f(server)
    }
}

pub(super) fn add(server: ArcServerInternal) -> anyhow::Result<()> {
    let name = server.name().clone();
    let mut sr = RUNTIME_SERVER_REGISTRY
        .lock()
        .map_err(|e| anyhow!("failed to lock server registry: {e}"))?;
    sr.add(name, server)
}

pub(super) fn del(name: &NodeName) {
    let mut sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.del(name);
}

pub(super) fn get_names() -> HashSet<NodeName> {
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.inner.keys().cloned().collect()
}

pub(super) fn get_config(name: &NodeName) -> Option<AnyServerConfig> {
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.inner.get(name).map(|server| server._clone_config())
}

pub(super) fn get_or_insert_default(name: &NodeName) -> ArcServer {
    let mut sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    sr.get_or_insert_default(name)
}

pub(super) fn reload_and_respawn(name: &NodeName, config: AnyServerConfig) -> anyhow::Result<()> {
    let mut sr = RUNTIME_SERVER_REGISTRY
        .lock()
        .map_err(|e| anyhow!("failed to lock server registry: {e}"))?;
    sr.reload_and_respawn(name, config)
}

pub(crate) fn foreach_online<F>(mut f: F)
where
    F: FnMut(&ArcServer),
{
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    for server in sr.inner.values() {
        let server: ArcServer = server.clone();
        f(&server);
    }
}

pub(super) fn foreach_online_internal<F>(mut f: F)
where
    F: FnMut(&ArcServerInternal),
{
    let sr = RUNTIME_SERVER_REGISTRY.lock().unwrap();
    for server in sr.inner.values() {
        f(server);
    }
}
