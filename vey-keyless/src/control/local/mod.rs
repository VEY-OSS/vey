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

    pub(super) async fn abort() {
        debug!("aborting unique controller");
        LocalController::abort_unique().await;
    }
}

impl DaemonController {
    pub fn start(program_name: String) -> anyhow::Result<impl Future + use<>> {
        LocalController::start_daemon(&program_name, crate::opts::daemon_group())
    }

    pub(super) async fn abort() {
        debug!("aborting daemon controller");
        LocalController::abort_daemon().await;
    }
}
