/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use log::debug;

use vey_daemon::control::LocalController;

pub struct UniqueController {}
pub struct DaemonController {}

impl UniqueController {
    pub fn start(program_name: &str) -> anyhow::Result<impl Future> {
        LocalController::start_unique(program_name, crate::opts::daemon_group())
    }

    pub(super) async fn abort_immediately() {
        debug!("aborting unique controller");
        LocalController::abort_unique().await;
    }

    pub(super) async fn abort_gracefully() {
        debug!("stopping all importers");
        crate::import::stop_all().await;
        debug!("stopped all importers");

        // TODO flush and stop all exporters

        UniqueController::abort_immediately().await
    }
}

impl DaemonController {
    pub fn start(program_name: String) -> anyhow::Result<impl Future> {
        LocalController::start_daemon(&program_name, crate::opts::daemon_group())
    }

    pub(super) async fn abort() {
        debug!("aborting daemon controller");
        LocalController::abort_daemon().await;
    }
}
