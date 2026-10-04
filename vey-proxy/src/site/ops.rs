/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashSet;

use anyhow::{Context, anyhow};
use async_recursion::async_recursion;
use log::{debug, warn};
use tokio::sync::Mutex;

use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::SiteGroup;
use super::registry;
use crate::config::site::SiteGroupConfig;

static SITE_GROUP_OPS_LOCK: Mutex<()> = Mutex::const_new(());

pub async fn load_all() -> anyhow::Result<()> {
    let _guard = SITE_GROUP_OPS_LOCK.lock().await;

    let mut new_names = HashSet::<NodeName>::new();

    let all_config = crate::config::site::get_all_sorted()?;
    for config in all_config {
        let name = config.name();
        new_names.insert(name.clone());
        match registry::get_config(name) {
            Some(old) => {
                debug!("reloading site group {name}");
                reload_old_unlocked(old, config.as_ref().clone()).await?;
                debug!("site group {name} reload OK");
            }
            None => {
                debug!("creating site group {name}");
                spawn_new_unlocked(config.as_ref().clone()).await?;
                debug!("site group {name} create OK");
            }
        }
    }

    for name in &registry::get_names() {
        if !new_names.contains(name) {
            debug!("deleting site group {name}");
            delete_existed_unlocked(name).await;
            debug!("site group {name} deleted");
        }
    }

    Ok(())
}

pub(crate) async fn reload(
    name: &NodeName,
    position: Option<YamlDocPosition>,
) -> anyhow::Result<()> {
    let _guard = SITE_GROUP_OPS_LOCK.lock().await;

    let old_config = match registry::get_config(name) {
        Some(config) => config,
        None => return Err(anyhow!("no site group with name {name} found")),
    };

    let position = match position {
        Some(position) => position,
        None => match old_config.position() {
            Some(position) => position,
            None => {
                return Err(anyhow!(
                    "no config position for site group {name} found, reload is not supported"
                ));
            }
        },
    };

    let position2 = position.clone();
    let config =
        tokio::task::spawn_blocking(move || crate::config::site::load_at_position(&position2))
            .await
            .map_err(|e| anyhow!("unable to join conf load task: {e}"))?
            .context(format!("unable to load conf at position {position}"))?;
    if name != config.name() {
        return Err(anyhow!(
            "site group at position {position} has name {}, while we expect {name}",
            config.name()
        ));
    }

    debug!("reloading site group {name} from position {position}");
    reload_old_unlocked(old_config, config).await?;
    debug!("site group {name} reload OK");
    Ok(())
}

async fn reload_old_unlocked(old: SiteGroupConfig, new: SiteGroupConfig) -> anyhow::Result<()> {
    let name = old.name().clone();
    let Some(old_group) = registry::get(&name) else {
        return Err(anyhow!("no site group with name {name} found"));
    };
    let new_group = old_group.reload(new)?;
    registry::add(name.clone(), new_group);
    update_dependency_to_site_group_unlocked(&name, "reloaded").await;
    crate::serve::update_dependency_to_site_group(&name, "reloaded").await;
    Ok(())
}

async fn delete_existed_unlocked(name: &NodeName) {
    const STATUS: &str = "deleted";

    registry::del(name);
    update_dependency_to_site_group_unlocked(name, STATUS).await;
    crate::serve::update_dependency_to_site_group(name, STATUS).await;
}

async fn reload_existed_unlocked(name: &NodeName) -> anyhow::Result<()> {
    const STATUS: &str = "reloaded";

    let Some(old_group) = registry::get(name) else {
        return Err(anyhow!("no site group with name {name} found"));
    };
    let config = old_group.clone_config();
    let new_group = old_group.reload(config)?;
    registry::add(name.clone(), new_group);
    update_dependency_to_site_group_unlocked(name, STATUS).await;
    crate::serve::update_dependency_to_site_group(name, STATUS).await;
    Ok(())
}

async fn spawn_new_unlocked(config: SiteGroupConfig) -> anyhow::Result<()> {
    const STATUS: &str = "spawned";

    let name = config.name().clone();
    let group = SiteGroup::new_with_config(config)?;
    registry::add(name.clone(), group);
    update_dependency_to_site_group_unlocked(&name, STATUS).await;
    crate::serve::update_dependency_to_site_group(&name, STATUS).await;
    Ok(())
}

#[async_recursion]
async fn update_dependency_to_site_group_unlocked(target: &NodeName, status: &str) {
    let mut names = Vec::<NodeName>::new();

    registry::foreach(|name, group| {
        if group._depend_on_site_group(target) {
            names.push(name.clone());
        }
    });

    debug!(
        "site group {target} changed({status}), will reload site group(s) {names:?} which depend on it"
    );
    for name in names.iter() {
        debug!("site group {name}: will reload as it depends on site group {target}");
        if let Err(e) = reload_existed_unlocked(name).await {
            warn!("failed to reload site group {name}: {e:?}");
        }
    }

    // finish those in the same level first, then go in depth
    for name in names.iter() {
        update_dependency_to_site_group_unlocked(name, "reloaded").await;
    }
}

pub(crate) async fn update_dependency_to_user_group(user_group: &NodeName, status: &str) {
    let _guard = SITE_GROUP_OPS_LOCK.lock().await;

    let mut names = Vec::<NodeName>::new();
    registry::foreach(|name, group| {
        if group.update_tenant_user_group_in_place(user_group) {
            names.push(name.clone());
        }
    });

    if names.is_empty() {
        return;
    }

    debug!(
        "user group {user_group} changed({status}), updated tenant_user_group on site group(s) {names:?}"
    );
}
