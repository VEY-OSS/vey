/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use clap::{Arg, ArgMatches, Command};

use vey_ctl::{CommandError, CommandResult};

use vey_iploc_proto::proc_capnp::proc_control;
use vey_iploc_proto::types_capnp::query_result;

use crate::common::parse_operation_result;

pub const COMMAND_VERSION: &str = "version";
pub const COMMAND_OFFLINE: &str = "offline";
pub const COMMAND_RELOAD: &str = "reload";
pub const COMMAND_RELOAD_COUNTRY: &str = "reload-country";
pub const COMMAND_RELOAD_ASN: &str = "reload-asn";
pub const COMMAND_CANCEL_SHUTDOWN: &str = "cancel-shutdown";
pub const COMMAND_QUERY: &str = "query";

const ARG_IP: &str = "ip";

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
            .about("Reload servers and GeoIP databases from the config file")
    }

    pub fn reload_country() -> Command {
        Command::new(COMMAND_RELOAD_COUNTRY)
            .about("Reload the country database from its configured file path")
    }

    pub fn reload_asn() -> Command {
        Command::new(COMMAND_RELOAD_ASN)
            .about("Reload the ASN database from its configured file path")
    }

    pub fn cancel_shutdown() -> Command {
        Command::new(COMMAND_CANCEL_SHUTDOWN)
            .about("Cancel the shutdown progress if the daemon is still in shutdown wait state")
    }

    pub fn query() -> Command {
        Command::new(COMMAND_QUERY)
            .about("Look up an IP address")
            .arg(Arg::new(ARG_IP).required(true).num_args(1).value_name("IP"))
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

pub async fn reload_country(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.reload_country_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn reload_asn(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.reload_asn_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn cancel_shutdown(client: &proc_control::Client) -> CommandResult<()> {
    let req = client.cancel_shutdown_request();
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}

pub async fn query(client: &proc_control::Client, args: &ArgMatches) -> CommandResult<()> {
    let ip = args.get_one::<String>(ARG_IP).unwrap();
    let mut req = client.query_request();
    req.get().set_ip(ip);
    let rsp = req.send().promise.await?;
    match rsp
        .get()?
        .get_result()?
        .which()
        .map_err(|e| anyhow::anyhow!("invalid query result: {e}"))?
    {
        query_result::Which::Location(location) => print_location(location?),
        query_result::Which::Err(err) => {
            let e = err?;
            Err(CommandError::api_error(e.get_code(), e.get_reason()?))
        }
    }
}

fn print_location(
    location: vey_iploc_proto::types_capnp::location_record::Reader<'_>,
) -> CommandResult<()> {
    print_field("network", location.get_network()?)?;
    print_field("country", location.get_country()?)?;
    print_field("continent", location.get_continent()?)?;
    if location.get_asn_set() {
        println!("asn: {}", location.get_asn());
    }
    print_field("isp_name", location.get_isp_name()?)?;
    print_field("isp_domain", location.get_isp_domain()?)?;
    Ok(())
}

fn print_field(name: &'static str, value: capnp::text::Reader<'_>) -> CommandResult<()> {
    let text = value.to_str().map_err(|e| CommandError::Utf8 {
        field: name,
        reason: e,
    })?;
    if !text.is_empty() {
        println!("{name}: {text}");
    }
    Ok(())
}
