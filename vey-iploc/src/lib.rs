/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use log::{debug, warn};

use vey_daemon::listen::RespondUdpRuntime;
use vey_daemon::server::ServerReloadCommand;

pub mod config;

mod build;

pub mod opts;
use opts::ProcArgs;

mod stat;

mod frontend;
use frontend::{Frontend, FrontendStats};

pub async fn run(proc_args: &ProcArgs) -> anyhow::Result<()> {
    let frontend_stats = Arc::new(FrontendStats::default());

    if let Some(stats_config) = vey_daemon::stat::config::get_global_stat_config() {
        stat::spawn_working_thread(stats_config, frontend_stats.clone())?;
    }

    let reload_sender = ServerReloadCommand::new_sender();
    let runtime = RespondUdpRuntime::new(
        Frontend::new(frontend_stats),
        proc_args.listen_config().clone(),
    );
    let tasks = runtime.run_all_instances(true, &reload_sender)?;

    if let Err(e) = tokio::signal::ctrl_c().await {
        warn!("failed to recv Ctrl-C signal: {e}");
    }
    debug!("received Ctrl-C signal, start shutdown now");
    let _ = reload_sender.send(ServerReloadCommand::QuitRuntime);

    for task in tasks {
        if let Err(e) = task.handle.await {
            warn!("iploc respond task ended with error: {e}");
        }
    }
    debug!("all requests served, quit now");
    Ok(())
}
