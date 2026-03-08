/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2025 ByteDance and/or its affiliates.
 */

use anyhow::Context;
use log::{debug, error, info};

use vey_daemon::control::{QuitAction, UpgradeAction};

use vey_statsd::opts::ProcArgs;

fn main() -> anyhow::Result<()> {
    let Some(proc_args) =
        vey_statsd::opts::parse_clap().context("failed to parse command line options")?
    else {
        return Ok(());
    };

    // set up process logger early, only proc args is used inside
    vey_daemon::log::process::setup(&proc_args.daemon_config);
    if proc_args.daemon_config.need_daemon_controller() {
        vey_statsd::control::UpgradeActor::connect_to_old_daemon();
    }

    let config_file = match vey_statsd::config::load() {
        Ok(c) => c,
        Err(e) => {
            vey_daemon::control::upgrade::cancel_old_shutdown();
            return Err(e.context(format!("failed to load config, opts: {:?}", &proc_args)));
        }
    };
    debug!("loaded config from {}", config_file.display());

    if proc_args.daemon_config.test_config {
        info!("the format of the config file is ok");
        return Ok(());
    }

    // enter daemon mode after config loaded
    #[cfg(unix)]
    vey_daemon::daemonize::check_enter(&proc_args.daemon_config)?;

    let _workers_guard =
        vey_daemon::runtime::worker::spawn_workers().context("failed to spawn workers")?;
    let ret = tokio_run(&proc_args);

    match ret {
        Ok(_) => Ok(()),
        Err(e) => {
            error!("{e:?}");
            Err(e)
        }
    }
}

fn tokio_run(args: &ProcArgs) -> anyhow::Result<()> {
    let rt = vey_daemon::runtime::config::get_runtime_config()
        .start()
        .context("failed to start runtime")?;
    rt.block_on(async {
        vey_daemon::runtime::set_main_handle();

        let ctl_thread_handler = vey_statsd::control::capnp::spawn_working_thread().await?;

        let unique_ctl = vey_statsd::control::UniqueController::start()
            .context("failed to start unique controller")?;
        if args.daemon_config.need_daemon_controller() {
            vey_daemon::control::upgrade::release_old_controller().await;
            let daemon_ctl = vey_statsd::control::DaemonController::start()
                .context("failed to start daemon controller")?;
            tokio::spawn(async move {
                daemon_ctl.await;
            });
        }
        vey_statsd::control::QuitActor::tokio_spawn_run();

        vey_statsd::signal::register().context("failed to setup signal handler")?;
        vey_daemon::control::panic::set_hook(&args.daemon_config);

        match load_and_spawn().await {
            Ok(_) => vey_daemon::control::upgrade::finish(),
            Err(e) => {
                vey_daemon::control::upgrade::cancel_old_shutdown();
                return Err(e);
            }
        }

        unique_ctl.await;

        vey_statsd::control::capnp::stop_working_thread();
        let _ = ctl_thread_handler.join();

        Ok(())
    })
}

async fn load_and_spawn() -> anyhow::Result<()> {
    vey_statsd::export::load_all()
        .await
        .context("failed to load all exporters")?;
    vey_statsd::collect::load_all()
        .await
        .context("failed to load all collectors")?;
    vey_statsd::import::spawn_all()
        .await
        .context("failed to spawn all importers")?;
    Ok(())
}
