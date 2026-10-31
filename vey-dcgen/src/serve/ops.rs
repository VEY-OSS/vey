/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use log::debug;
use tokio::sync::Mutex;

use vey_types::metrics::NodeName;

use super::registry;
use crate::config::server::{AnyServerConfig, ServerConfigDiffAction};

static SERVER_OPS_LOCK: Mutex<()> = Mutex::const_new(());

/// Shutdown checks this often. A datagram listener stops within the reuseport drain window.
const QUIT_POLL_INTERVAL: Duration = Duration::from_millis(200);

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

pub async fn spawn_all() -> anyhow::Result<()> {
    let _guard = SERVER_OPS_LOCK.lock().await;

    let mut new_names = HashSet::new();
    for config in crate::config::server::get_all() {
        let name = config.name().clone();
        new_names.insert(name.clone());
        match registry::get_config(&name) {
            Some(old) => {
                debug!("reloading server {name}");
                reload_old_unlocked(old, config)?;
                debug!("server {name} reload OK");
            }
            None => {
                debug!("creating server {name}");
                spawn_new_unlocked(config)?;
                debug!("server {name} create OK");
            }
        }
    }

    for name in registry::get_names() {
        if !new_names.contains(&name) {
            debug!("deleting server {name}");
            registry::del(&name);
            debug!("server {name} deleted");
        }
    }
    Ok(())
}

fn reload_old_unlocked(old: AnyServerConfig, new: AnyServerConfig) -> anyhow::Result<()> {
    let name = old.name().clone();
    match old.diff_action(&new) {
        ServerConfigDiffAction::NoAction => {
            debug!("server {name} reload: no action is needed");
            Ok(())
        }
        ServerConfigDiffAction::SpawnNew => {
            debug!("server {name} reload: will create a new server");
            spawn_new_unlocked(new)
        }
        ServerConfigDiffAction::ReloadAndRespawn => {
            debug!("server {name} reload: will respawn with old stats");
            registry::reload_and_respawn(&name, new)
        }
    }
}

fn spawn_new_unlocked(config: AnyServerConfig) -> anyhow::Result<()> {
    let server = match config {
        AnyServerConfig::Dummy(config) => super::dummy::DummyServer::prepare_initial(config)?,
        AnyServerConfig::Udp(config) => super::udp::UdpServer::prepare_initial(config)?,
        #[cfg(unix)]
        AnyServerConfig::Unix(config) => super::unix::UnixServer::prepare_initial(config)?,
    };
    registry::add(server)
}

pub(crate) async fn stop_all() {
    let _guard = SERVER_OPS_LOCK.lock().await;

    registry::foreach_online_internal(|server| {
        server._abort_runtime();
        registry::add_offline(Arc::clone(server));
    });
}

pub(crate) async fn wait_all_tasks<F>(wait_timeout: Duration, quit_timeout: Duration, on_timeout: F)
where
    F: Fn(&NodeName, i32),
{
    let loop_wait = async {
        loop {
            let mut has_pending = false;

            registry::foreach_offline(|server| {
                if server.alive_count() > 0 || server.listen_stats().is_running() {
                    has_pending = true;
                }
            });

            if !has_pending {
                if let Some(stat_config) = vey_daemon::stat::config::get_global_stat_config() {
                    tokio::time::sleep(stat_config.emit_interval * 2).await;
                }
                break;
            }

            tokio::time::sleep(QUIT_POLL_INTERVAL).await;
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

    debug!("will wait {quit_timeout:?} for remaining tasks");
    if tokio::time::timeout(quit_timeout, &mut loop_wait)
        .await
        .is_err()
    {
        registry::foreach_offline(|server| {
            let mut left = server.alive_count();
            if left == 0 && server.listen_stats().is_running() {
                left = 1;
            }
            if left > 0 {
                on_timeout(server.name(), left);
            }
        });
    }
}
