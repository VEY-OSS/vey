/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::Context;
use log::{debug, error, info};

use vey_daemon::control::{QuitAction, UpgradeAction};
#[cfg(feature = "jemalloc")]
use vey_jemalloc::Jemalloc;

use vey_gateway::opts::ProcArgs;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "openssl-probe")]
    unsafe {
        openssl_probe::try_init_openssl_env_vars();
    }
    openssl::init();

    vey_rustls_provider::install_default()?;

    let Some(proc_args) =
        vey_gateway::opts::parse_clap().context("failed to parse command line options")?
    else {
        return Ok(());
    };

    // set up process logger early, only proc args is used inside
    vey_daemon::log::process::setup(&proc_args.daemon_config);
    if proc_args.daemon_config.need_daemon_controller() {
        vey_gateway::control::UpgradeActor::connect_to_old_daemon();
    }

    let config_file = match vey_gateway::config::load() {
        Ok(c) => c,
        Err(e) => {
            vey_daemon::control::upgrade::cancel_old_shutdown();
            return Err(e.context(format!("failed to load config, opts: {proc_args:?}")));
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

    let stat_join = if let Some(stat_config) = vey_daemon::stat::config::get_global_stat_config() {
        Some(
            vey_gateway::stat::spawn_working_threads(stat_config)
                .context("failed to start stat thread")?,
        )
    } else {
        None
    };

    let _workers_guard =
        vey_daemon::runtime::worker::spawn_workers().context("failed to spawn workers")?;
    let ret = tokio_run(&proc_args);

    if let Some(handlers) = stat_join {
        vey_gateway::stat::stop_working_threads();
        for handle in handlers {
            let _ = handle.join();
        }
    }

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

        let ctl_thread_handler = vey_gateway::control::capnp::spawn_working_thread().await?;

        let unique_ctl = vey_gateway::control::UniqueController::start()
            .context("failed to start unique controller")?;
        if args.daemon_config.need_daemon_controller() {
            vey_daemon::control::upgrade::release_old_controller().await;
            let daemon_ctl = vey_gateway::control::DaemonController::start()
                .context("failed to start daemon controller")?;
            tokio::spawn(async move {
                daemon_ctl.await;
            });
        }
        vey_gateway::control::QuitActor::tokio_spawn_run();

        vey_gateway::signal::register().context("failed to setup signal handler")?;
        vey_daemon::control::panic::set_hook(&args.daemon_config);

        match load_and_spawn().await {
            Ok(_) => vey_daemon::control::upgrade::finish(),
            Err(e) => {
                vey_daemon::control::upgrade::cancel_old_shutdown();
                return Err(e);
            }
        }

        unique_ctl.await;

        vey_gateway::control::capnp::stop_working_thread();
        let _ = ctl_thread_handler.join();

        Ok(())
    })
}

async fn load_and_spawn() -> anyhow::Result<()> {
    vey_gateway::discover::load_all()
        .await
        .context("failed to load all discovers")?;
    vey_gateway::backend::load_all()
        .await
        .context("failed to load all connectors")?;
    vey_gateway::serve::spawn_offline_clean();
    vey_gateway::serve::spawn_all()
        .await
        .context("failed to spawn all servers")?;
    Ok(())
}
