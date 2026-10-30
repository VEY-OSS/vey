/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::Context;
use log::{info, warn};
use tokio::sync::Mutex;

use vey_daemon::signal::AsyncSignalAction;

static RELOAD_MUTEX: Mutex<()> = Mutex::const_new(());

pub(crate) async fn reload_country() -> anyhow::Result<()> {
    reload_one_db(
        "country",
        crate::config::geoip::reload_country,
        crate::geoip::load_country,
    )
    .await
}

pub(crate) async fn reload_asn() -> anyhow::Result<()> {
    reload_one_db(
        "asn",
        crate::config::geoip::reload_asn,
        crate::geoip::load_asn,
    )
    .await
}

async fn reload_one_db(
    kind: &'static str,
    reload: fn() -> anyhow::Result<()>,
    publish: fn(),
) -> anyhow::Result<()> {
    let _guard = RELOAD_MUTEX.lock().await;
    info!("reloading {kind} database");

    let reload = tokio::task::spawn_blocking(move || {
        reload()?;
        publish();
        Ok(())
    });
    match reload
        .await
        .map_err(|e| anyhow::anyhow!("failed to join reload task: {e}"))?
    {
        Ok(()) => {
            info!("{kind} database reload finished");
            Ok(())
        }
        Err(e) => {
            warn!("{kind} database reload error: {e:?}");
            Err(e)
        }
    }
}

pub(crate) async fn reload() -> anyhow::Result<()> {
    let _guard = RELOAD_MUTEX.lock().await;
    info!("reloading config");

    match reload_locked().await {
        Ok(_) => {
            info!("reload finished");
            Ok(())
        }
        Err(e) => {
            warn!("reload error: {e:?}");
            warn!("reload aborted");
            Err(e)
        }
    }
}

async fn reload_locked() -> anyhow::Result<()> {
    crate::config::reload()
        .await
        .context("failed to reload config")?;
    crate::geoip::load();
    crate::serve::spawn_all()
        .await
        .context("failed to reload all servers")?;
    Ok(())
}

#[derive(Clone, Copy)]
struct QuitAction {}

impl AsyncSignalAction for QuitAction {
    async fn run(&self) {
        vey_daemon::control::quit::trigger_force_shutdown();
    }
}

#[cfg(unix)]
mod unix {
    use vey_daemon::signal::AsyncSignalAction;

    #[derive(Clone, Copy)]
    struct OfflineAction {}

    impl AsyncSignalAction for OfflineAction {
        async fn run(&self) {
            vey_daemon::control::quit::start_graceful_shutdown().await;
        }
    }

    #[derive(Clone, Copy)]
    struct ReloadAction {}

    impl AsyncSignalAction for ReloadAction {
        async fn run(&self) {
            let _ = super::reload().await;
        }
    }

    pub(super) fn register() -> anyhow::Result<()> {
        vey_daemon::signal::register_reload(ReloadAction {})?;
        vey_daemon::signal::register_offline(OfflineAction {})?;
        Ok(())
    }
}

pub fn register() -> anyhow::Result<()> {
    #[cfg(unix)]
    unix::register()?;
    vey_daemon::signal::register_quit(QuitAction {})
}
