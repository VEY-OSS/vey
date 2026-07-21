/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::Context;
use log::{info, warn};
use tokio::sync::Mutex;

use vey_daemon::signal::AsyncSignalAction;

static RELOAD_MUTEX: Mutex<()> = Mutex::const_new(());

pub(super) async fn reload() -> anyhow::Result<()> {
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

    crate::collect::load_all()
        .await
        .context("failed to reload all collectors")?;
    crate::import::spawn_all()
        .await
        .context("failed to reload all importers")?;
    crate::export::load_all()
        .await
        .context("failed to reload all exporters")?;

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
