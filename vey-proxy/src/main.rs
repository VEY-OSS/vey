/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use anyhow::Context;
use log::{debug, error, info};

use g3_daemon::control::{QuitAction, UpgradeAction};

use vey_proxy::opts::ProcArgs;

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "openssl-probe")]
    unsafe {
        openssl_probe::try_init_openssl_env_vars();
    }
    openssl::init();

    #[cfg(any(feature = "rustls-aws-lc", feature = "rustls-aws-lc-fips"))]
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .unwrap();
    #[cfg(feature = "rustls-ring")]
    rustls::crypto::ring::default_provider()
        .install_default()
        .unwrap();
    #[cfg(not(any(
        feature = "rustls-aws-lc",
        feature = "rustls-aws-lc-fips",
        feature = "rustls-ring"
    )))]
    compile_error!("either rustls-aws-lc or rustls-ring should be enabled");

    let Some(proc_args) =
        vey_proxy::opts::parse_clap().context("failed to parse command line options")?
    else {
        return Ok(());
    };

    // set up process logger early, only proc args is used inside
    g3_daemon::log::process::setup(&proc_args.daemon_config);
    if proc_args.daemon_config.need_daemon_controller() {
        vey_proxy::control::UpgradeActor::connect_to_old_daemon();
    }

    let config_file = match vey_proxy::config::load() {
        Ok(c) => c,
        Err(e) => {
            g3_daemon::control::upgrade::cancel_old_shutdown();
            return Err(e.context(format!("failed to load config, opts: {:?}", &proc_args)));
        }
    };
    debug!("loaded config from {}", config_file.display());

    if proc_args.daemon_config.test_config {
        info!("the format of the config file is ok");
        return Ok(());
    }
    if proc_args.output_graphviz_graph {
        let content = vey_proxy::config::graphviz_graph()?;
        println!("{content}");
        return Ok(());
    }
    if proc_args.output_mermaid_graph {
        let content = vey_proxy::config::mermaid_graph()?;
        println!("{content}");
        return Ok(());
    }
    if proc_args.output_plantuml_graph {
        let content = vey_proxy::config::plantuml_graph()?;
        println!("{content}");
        return Ok(());
    }

    // enter daemon mode after config loaded
    #[cfg(unix)]
    g3_daemon::daemonize::check_enter(&proc_args.daemon_config)?;

    let stat_join = if let Some(stat_config) = g3_daemon::stat::config::get_global_stat_config() {
        Some(
            vey_proxy::stat::spawn_working_threads(stat_config)
                .context("failed to start stat thread")?,
        )
    } else {
        None
    };

    let _workers_guard =
        g3_daemon::runtime::worker::spawn_workers().context("failed to spawn workers")?;
    let ret = tokio_run(&proc_args);

    if let Some(handlers) = stat_join {
        vey_proxy::stat::stop_working_threads();
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
    let rt = g3_daemon::runtime::config::get_runtime_config()
        .start()
        .context("failed to start runtime")?;
    rt.block_on(async {
        g3_daemon::runtime::set_main_handle();

        let ctl_thread_handler = vey_proxy::control::capnp::spawn_working_thread().await?;

        let unique_ctl = vey_proxy::control::UniqueController::start()
            .context("failed to start unique controller")?;
        if args.daemon_config.need_daemon_controller() {
            g3_daemon::control::upgrade::release_old_controller().await;
            let daemon_ctl = vey_proxy::control::DaemonController::start()
                .context("failed to start daemon controller")?;
            tokio::spawn(async move {
                daemon_ctl.await;
            });
        }
        vey_proxy::control::QuitActor::tokio_spawn_run();

        vey_proxy::signal::register().context("failed to setup signal handler")?;
        g3_daemon::control::panic::set_hook(&args.daemon_config);

        if let Some(stats) = vey_io_ext::spawn_limit_schedule_runtime().await {
            g3_daemon::runtime::metrics::add_tokio_stats(stats, "limit-schedule".to_string());
        }
        if let Some(stats) = g3_cert_agent::spawn_cert_generate_runtime().await {
            g3_daemon::runtime::metrics::add_tokio_stats(stats, "cert-generate".to_string());
        }
        if let Some(stats) = vey_ip_locate::spawn_ip_locate_runtime().await {
            g3_daemon::runtime::metrics::add_tokio_stats(stats, "ip-locate".to_string());
        }

        match load_and_spawn().await {
            Ok(_) => g3_daemon::control::upgrade::finish(),
            Err(e) => {
                g3_daemon::control::upgrade::cancel_old_shutdown();
                return Err(e);
            }
        }

        unique_ctl.await;

        vey_io_ext::close_limit_schedule_runtime();
        g3_cert_agent::close_cert_generate_runtime();
        vey_ip_locate::close_ip_locate_runtime();
        vey_proxy::control::capnp::stop_working_thread();
        let _ = ctl_thread_handler.join();

        Ok(())
    })
}

async fn load_and_spawn() -> anyhow::Result<()> {
    vey_proxy::resolve::spawn_all()
        .await
        .context("failed to spawn all resolvers")?;
    vey_proxy::escape::load_all()
        .await
        .context("failed to load all escapers")?;
    vey_proxy::auth::load_all()
        .await
        .context("failed to load all user groups")?;
    vey_proxy::audit::load_all()
        .await
        .context("failed to load all auditors")?;
    vey_proxy::serve::spawn_offline_clean();
    vey_proxy::serve::spawn_all()
        .await
        .context("failed to spawn all servers")?;
    Ok(())
}
