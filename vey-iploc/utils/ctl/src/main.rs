/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use clap::Command;

use vey_ctl::{CommandError, DaemonCtlArgs, DaemonCtlArgsExt};

use vey_iploc_proto::proc_capnp::proc_control;

mod common;
mod proc;

fn build_cli_args() -> Command {
    Command::new(env!("CARGO_PKG_NAME"))
        .append_daemon_ctl_args()
        .subcommand(proc::commands::version())
        .subcommand(proc::commands::offline())
        .subcommand(proc::commands::reload())
        .subcommand(proc::commands::reload_country())
        .subcommand(proc::commands::reload_asn())
        .subcommand(proc::commands::cancel_shutdown())
        .subcommand(proc::commands::query())
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
    let daemon_name = bin_name.strip_suffix("-ctl").unwrap_or("vey-iploc");
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

            let (subcommand, sub_args) = args.subcommand().unwrap();
            match subcommand {
                proc::COMMAND_VERSION => proc::version(&proc_control).await,
                proc::COMMAND_OFFLINE => proc::offline(&proc_control).await,
                proc::COMMAND_RELOAD => proc::reload(&proc_control).await,
                proc::COMMAND_RELOAD_COUNTRY => proc::reload_country(&proc_control).await,
                proc::COMMAND_RELOAD_ASN => proc::reload_asn(&proc_control).await,
                proc::COMMAND_CANCEL_SHUTDOWN => proc::cancel_shutdown(&proc_control).await,
                proc::COMMAND_QUERY => proc::query(&proc_control, sub_args).await,
                _ => Err(CommandError::Cli(anyhow!(
                    "unsupported command {subcommand}"
                ))),
            }
        })
        .await
        .map_err(anyhow::Error::new)
}
