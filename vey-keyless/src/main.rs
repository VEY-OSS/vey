/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use anyhow::{Context, anyhow};
use log::{debug, error, info, warn};

use vey_daemon::control::{QuitAction, UpgradeAction};

use vey_keyless::opts::ProcArgs;

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "openssl-probe")]
    unsafe {
        openssl_probe::try_init_openssl_env_vars();
    }
    openssl::init();

    let Some(proc_args) =
        vey_keyless::opts::parse_clap().context("failed to parse command line options")?
    else {
        return Ok(());
    };

    if let Some(cpu_affinity) = &proc_args.core_affinity
        && let Err(e) = cpu_affinity.apply_to_local_thread()
    {
        warn!("failed to apply cpu affinity: {e}");
    }

    // set up process logger early, only proc args is used inside
    vey_daemon::log::process::setup(&proc_args.daemon_config);
    if proc_args.daemon_config.need_daemon_controller() {
        vey_keyless::control::UpgradeActor::connect_to_old_daemon();
    }

    vey_daemon::runtime::config::set_default_thread_number(0); // default to use current thread
    let config_file = match vey_keyless::config::load() {
        Ok(c) => c,
        Err(e) => {
            vey_daemon::control::upgrade::cancel_old_shutdown();
            return Err(e.context("failed to load config"));
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
            vey_keyless::stat::spawn_working_threads(stat_config)
                .context("failed to start stat thread")?,
        )
    } else {
        None
    };

    let _workers_guard =
        vey_daemon::runtime::worker::spawn_workers().context("failed to spawn workers")?;
    let ret = tokio_run(&proc_args);

    if let Some(handlers) = stat_join {
        vey_keyless::stat::stop_working_threads();
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
    let mut rt_builder = vey_daemon::runtime::config::get_runtime_config().builder();
    #[cfg(feature = "openssl-async-job")]
    if let Some(async_job_size) = args.openssl_async_job {
        rt_builder.on_thread_start(move || {
            if let Err(e) =
                vey_openssl::async_job::async_thread_init(async_job_size, async_job_size)
            {
                warn!("failed to init {async_job_size} openssl async jobs: {e}");
            }
        });
        rt_builder.on_thread_stop(vey_openssl::async_job::async_thread_cleanup);
    }
    let rt = rt_builder
        .build()
        .map_err(|e| anyhow!("failed to start main runtime: {e}"))?;
    rt.block_on(async {
        vey_daemon::runtime::set_main_handle();

        let ctl_thread_handler = vey_keyless::control::capnp::spawn_working_thread().await?;

        let unique_controller = vey_keyless::control::UniqueController::create()
            .context("failed to create unique controller")?;
        let unique_ctl_path = unique_controller.listen_path();
        let unique_ctl = unique_controller
            .start()
            .context("failed to start unique controller")?;
        if args.daemon_config.need_daemon_controller() {
            vey_daemon::control::upgrade::release_old_controller().await;
            let daemon_ctl = vey_keyless::control::DaemonController::start()
                .context("failed to start daemon controller")?;
            tokio::spawn(async move {
                daemon_ctl.await;
            });
        }
        vey_keyless::control::QuitActor::tokio_spawn_run();

        vey_keyless::signal::register().context("failed to setup signal handler")?;
        vey_daemon::control::panic::set_hook(&args.daemon_config);

        match load_and_spawn(unique_ctl_path).await {
            Ok(_) => vey_daemon::control::upgrade::finish(),
            Err(e) => {
                vey_daemon::control::upgrade::cancel_old_shutdown();
                return Err(e);
            }
        }

        unique_ctl.await;

        vey_keyless::control::capnp::stop_working_thread();
        let _ = ctl_thread_handler.join();

        Ok(())
    })
}

async fn load_and_spawn(unique_ctl_path: String) -> anyhow::Result<()> {
    vey_keyless::store::load_all()
        .await
        .context("failed to load all key stores")?;

    vey_daemon::runtime::worker::foreach(|r| vey_keyless::backend::create(r.id, &r.handle))?;

    vey_keyless::serve::spawn_offline_clean();
    if let Some(config) = vey_daemon::register::get_pre_config() {
        vey_keyless::serve::create_all_stopped().await?;
        tokio::spawn(async move {
            if let Err(e) = vey_keyless::register::startup(config, unique_ctl_path).await {
                warn!("register failed: {e:?}");
                vey_daemon::control::quit::trigger_force_shutdown();
            } else if let Err(e) = vey_keyless::serve::start_all_stopped().await {
                warn!("failed to start all servers: {e:?}");
                vey_daemon::control::quit::trigger_force_shutdown();
            }
        });
    } else {
        vey_keyless::serve::spawn_all()
            .await
            .context("failed to start all servers")?;
    }
    Ok(())
}
