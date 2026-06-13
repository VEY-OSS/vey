/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use clap::Command;

use vey_ctl::{CommandError, DaemonCtlArgs, DaemonCtlArgsExt};

use vey_gateway_proto::proc_capnp::proc_control;

mod common;
mod proc;

mod backend;
mod server;

fn build_cli_args() -> Command {
    Command::new(env!("CARGO_PKG_NAME"))
        .append_daemon_ctl_args()
        .subcommand(proc::commands::version())
        .subcommand(proc::commands::offline())
        .subcommand(proc::commands::cancel_shutdown())
        .subcommand(proc::commands::force_quit())
        .subcommand(proc::commands::force_quit_all())
        .subcommand(proc::commands::list())
        .subcommand(proc::commands::reload_server())
        .subcommand(proc::commands::reload_discover())
        .subcommand(proc::commands::reload_backend())
        .subcommand(server::command())
        .subcommand(backend::command())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args_parser = build_cli_args();
    let args = args_parser.get_matches_mut();

    let mut ctl_opts = DaemonCtlArgs::parse_clap(&args);
    if ctl_opts.generate_shell_completion(build_cli_args) {
        return Ok(());
    }

    let bin_name = args_parser.get_bin_name().unwrap_or(args_parser.get_name());
    let daemon_name = bin_name.strip_suffix("-ctl").unwrap_or("vey-gateway");
    let (rpc_system, proc_control) = ctl_opts
        .connect_rpc::<proc_control::Client>(daemon_name)
        .await?;

    tokio::task::LocalSet::new()
        .run_until(async move {
            tokio::task::spawn_local(async move {
                rpc_system
                    .await
                    .map_err(|e| eprintln!("rpc system error: {e:?}"))
            });

            let (subcommand, args) = args.subcommand().unwrap();
            match subcommand {
                proc::COMMAND_VERSION => proc::version(&proc_control).await,
                proc::COMMAND_OFFLINE => proc::offline(&proc_control).await,
                proc::COMMAND_CANCEL_SHUTDOWN => proc::cancel_shutdown(&proc_control).await,
                proc::COMMAND_FORCE_QUIT => proc::force_quit(&proc_control, args).await,
                proc::COMMAND_FORCE_QUIT_ALL => proc::force_quit_all(&proc_control).await,
                proc::COMMAND_LIST => proc::list(&proc_control, args).await,
                proc::COMMAND_RELOAD_SERVER => proc::reload_server(&proc_control, args).await,
                proc::COMMAND_RELOAD_DISCOVER => proc::reload_discover(&proc_control, args).await,
                proc::COMMAND_RELOAD_BACKEND => proc::reload_backend(&proc_control, args).await,
                server::COMMAND => server::run(&proc_control, args).await,
                backend::COMMAND => backend::run(&proc_control, args).await,
                _ => Err(CommandError::Cli(anyhow!(
                    "unsupported command {subcommand}"
                ))),
            }
        })
        .await
        .map_err(anyhow::Error::new)
}
