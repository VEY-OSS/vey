/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashSet;
use std::time::Duration;

use anyhow::anyhow;
use log::debug;
use tokio::sync::Mutex;

use vey_types::metrics::NodeName;

use super::{ArcKeyServer, ArcKeyServerInternal, registry};
use crate::config::server::{AnyKeyServerConfig, KeyServerConfigDiffAction};

static SERVER_OPS_LOCK: Mutex<()> = Mutex::const_new(());

pub fn spawn_offline_clean() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.tick().await;
        loop {
            registry::retain_offline();
            interval.tick().await;
        }
    });
}

pub async fn create_all_stopped() -> anyhow::Result<()> {
    let _guard = SERVER_OPS_LOCK.lock().await;

    let all_config = crate::config::server::get_all();
    for config in all_config {
        let name = config.name();
        debug!("creating server {name}");
        spawn_new_lazy_unlocked(config.as_ref().clone())?;
        debug!("server {name} create OK");
    }
    Ok(())
}

pub async fn start_all_stopped() -> anyhow::Result<()> {
    registry::foreach_start_runtime()
}

pub async fn spawn_all() -> anyhow::Result<()> {
    let _guard = SERVER_OPS_LOCK.lock().await;

    let mut new_names = HashSet::<NodeName>::new();

    let all_config = crate::config::server::get_all();
    for config in all_config {
        let name = config.name();
        new_names.insert(name.clone());
        match registry::get_config(name) {
            Some(old) => {
                debug!("reloading server {name}");
                reload_old_unlocked(&old, config.as_ref().clone())?;
                debug!("server {name} reload OK");
            }
            None => {
                debug!("creating server {name}");
                spawn_new_unlocked(config.as_ref().clone())?;
                debug!("server {name} create OK");
            }
        }
    }

    for name in &registry::get_names() {
        if !new_names.contains(name) {
            debug!("deleting server {name}");
            delete_existed_unlocked(name);
            debug!("server {name} deleted");
        }
    }

    Ok(())
}

pub async fn stop_all() {
    let _guard = SERVER_OPS_LOCK.lock().await;

    registry::move_all_offline();
}

pub(crate) fn get_server(name: &NodeName) -> anyhow::Result<ArcKeyServer> {
    match registry::get_server(name) {
        Some(server) => Ok(server),
        None => Err(anyhow!("no server named {name} found")),
    }
}

fn update_dependency_to_server_unlocked(target: &NodeName, status: &str) {
    let mut servers = Vec::<ArcKeyServerInternal>::new();

    registry::foreach_online_internal(|_name, server| {
        if server._depend_on_server(target) {
            servers.push(server.clone());
        }
    });

    if servers.is_empty() {
        return;
    }

    debug!(
        "server {target} changed({status}), will reload {} server(s)",
        servers.len()
    );
    for server in servers.iter() {
        debug!(
            "server {}: will reload next servers as it's using server {target}",
            server.name()
        );
        server._update_next_server_in_place();
    }
}

fn reload_old_unlocked(old: &AnyKeyServerConfig, new: AnyKeyServerConfig) -> anyhow::Result<()> {
    let name = old.name().clone();
    match old.diff_action(&new) {
        KeyServerConfigDiffAction::NoAction => {
            debug!("server {name} reload: no action is needed");
            Ok(())
        }
        KeyServerConfigDiffAction::SpawnNew => {
            debug!("server {name} reload: will create a new server");
            spawn_new_unlocked(new)
        }
        KeyServerConfigDiffAction::ReloadAndRespawn => {
            debug!("server {name} reload: will respawn with old stats");
            registry::reload_and_respawn(&name, new)?;
            update_dependency_to_server_unlocked(&name, "reloaded");
            Ok(())
        }
    }
}

fn delete_existed_unlocked(name: &NodeName) {
    registry::del(name);
    update_dependency_to_server_unlocked(name, "deleted");
}

fn spawn_new_unlocked(config: AnyKeyServerConfig) -> anyhow::Result<()> {
    let name = config.name().clone();
    let server = super::new_server(config)?;
    registry::add(name.clone(), server)?;
    update_dependency_to_server_unlocked(&name, "spawned");
    Ok(())
}

fn spawn_new_lazy_unlocked(config: AnyKeyServerConfig) -> anyhow::Result<()> {
    let name = config.name().clone();
    let server = super::new_server(config)?;
    registry::add_lazy(name.clone(), server);
    update_dependency_to_server_unlocked(&name, "spawned");
    Ok(())
}

pub(crate) async fn wait_all_tasks<F>(wait_timeout: Duration, quit_timeout: Duration, on_timeout: F)
where
    F: Fn(&NodeName, i32),
{
    let loop_wait = async {
        loop {
            let mut has_pending = false;

            registry::foreach_offline(|server| {
                if server.alive_count() > 0 {
                    has_pending = true;
                }
            });

            if !has_pending {
                if let Some(stat_config) = vey_daemon::stat::config::get_global_stat_config() {
                    // sleep more time for flushing metrics
                    tokio::time::sleep(stat_config.emit_interval * 2).await;
                }
                break;
            }

            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    };

    tokio::pin!(loop_wait);

    debug!("will wait {wait_timeout:?} for all tasks to be finished");
    if tokio::time::timeout(wait_timeout, &mut loop_wait)
        .await
        .is_ok()
    {
        return;
    }

    // enable force_quit and wait more time
    force_quit_offline_servers();

    debug!("will wait {quit_timeout:?} for all tasks to force quit");
    if tokio::time::timeout(quit_timeout, &mut loop_wait)
        .await
        .is_err()
    {
        registry::foreach_offline(|server| {
            on_timeout(server.name(), server.alive_count());
        });
    }
}

pub(crate) fn force_quit_offline_servers() {
    registry::foreach_offline(|server| {
        server.quit_policy().set_force_quit();
    });
}
