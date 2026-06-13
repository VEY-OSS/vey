/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use log::debug;

use vey_daemon::control::LocalController;

pub struct UniqueController {
    inner: LocalController,
}
pub struct DaemonController {}

impl UniqueController {
    pub fn create(program_name: &str) -> anyhow::Result<Self> {
        let controller = LocalController::create_unique(program_name, crate::opts::daemon_group())?;
        Ok(UniqueController { inner: controller })
    }

    pub fn start(self) -> anyhow::Result<impl Future> {
        self.inner.start_as_unique()
    }

    #[inline]
    pub fn listen_path(&self) -> String {
        self.inner.listen_path()
    }

    async fn abort(force: bool) {
        // make sure we always shut down protected io
        // crate::control::disable_protected_io().await;

        debug!("stopping all servers");
        crate::serve::stop_all().await;
        debug!("stopped all servers");

        if !force {
            let delay = vey_daemon::runtime::config::get_task_wait_delay();
            debug!("will delay {delay:?} before waiting tasks");
            tokio::time::sleep(delay).await;
            let wait = vey_daemon::runtime::config::get_task_wait_timeout();
            let quit = vey_daemon::runtime::config::get_task_quit_timeout();

            crate::serve::wait_all_tasks(wait, quit, |name, left| {
                debug!("{left} tasks left on server {name}");
            })
            .await;
        }

        debug!("aborting unique controller");
        LocalController::abort_unique().await;
    }

    pub(super) async fn abort_immediately() {
        UniqueController::abort(true).await
    }

    pub(super) async fn abort_gracefully() {
        UniqueController::abort(false).await
    }
}

impl DaemonController {
    pub fn start(program_name: String) -> anyhow::Result<impl Future + use<>> {
        LocalController::start_daemon(&program_name, crate::opts::daemon_group())
    }

    pub(super) async fn abort() {
        // shutdown protected io before going to offline
        // crate::control::disable_protected_io().await;

        debug!("aborting daemon controller");
        LocalController::abort_daemon().await;
    }
}
