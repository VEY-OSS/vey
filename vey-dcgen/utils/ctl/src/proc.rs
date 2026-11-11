/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use clap::{Arg, ArgMatches, Command};

use vey_ctl::CommandResult;

use vey_dcgen_proto::proc_capnp::proc_control;

use crate::common::parse_operation_result;

pub const COMMAND_VERSION: &str = "version";
pub const COMMAND_OFFLINE: &str = "offline";
pub const COMMAND_RELOAD: &str = "reload";
pub const COMMAND_RELOAD_ISSUER: &str = "reload-issuer";
pub const COMMAND_RELOAD_SITE: &str = "reload-site";
pub const COMMAND_CANCEL_SHUTDOWN: &str = "cancel-shutdown";

pub mod commands {
    use super::*;

    pub fn version() -> Command {
        Command::new(COMMAND_VERSION)
    }

    pub fn offline() -> Command {
        Command::new(COMMAND_OFFLINE).about("Put this daemon into offline mode")
    }

    pub fn reload() -> Command {
        Command::new(COMMAND_RELOAD)
            .about("Reload servers, the issuer, and site certificates from the config file")
    }

    pub fn reload_issuer() -> Command {
        Command::new(COMMAND_RELOAD_ISSUER).about("Reload the issuer from the main config")
    }

    pub fn reload_site() -> Command {
        Command::new(COMMAND_RELOAD_SITE)
            .about("Reload one site from its recorded file when the loaded id matches")
            .arg(
                Arg::new("site")
                    .value_name("SITE-ID")
                    .required(true)
                    .help("Site id to reload"),
            )
    }

    pub fn cancel_shutdown() -> Command {
        Command::new(COMMAND_CANCEL_SHUTDOWN)
            .about("Cancel the shutdown progress if the daemon is still in shutdown wait state")
    }
}

pub async fn version(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.version_request();
    let rsp = req.send().promise.await?;
    vey_ctl::print_version(rsp.get()?.get_version()?)
}

pub async fn offline(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.offline_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn reload(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.reload_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn reload_issuer(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.reload_issuer_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn reload_site(client: &proc_control::Client, args: &ArgMatches) -> CommandResult<()> {
    let name = args.get_one::<String>("site").unwrap();
    let mut req = client.reload_site_request();
    req.get().set_name(name);
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn cancel_shutdown(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.cancel_shutdown_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}
