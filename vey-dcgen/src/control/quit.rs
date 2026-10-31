/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use log::debug;

use vey_daemon::control::quit::QuitAction;

use super::local::{DaemonController, UniqueController};

#[derive(Default)]
pub struct QuitActor {}

impl QuitAction for QuitActor {
    async fn release_controller(&self) {
        DaemonController::abort().await;
    }

    fn resume_controller(&self, program_name: String) -> anyhow::Result<()> {
        let daemon_ctl = DaemonController::start(program_name)?;
        tokio::spawn(async move {
            daemon_ctl.await;
        });
        Ok(())
    }

    async fn start_shutdown(&self) {
        debug!("stopping all servers");
        crate::serve::stop_all().await;
        debug!("stopped all servers");
    }

    async fn wait_graceful_shutdown(&self) {
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

    async fn shutdown(&self) {
        UniqueController::abort().await
    }
}
