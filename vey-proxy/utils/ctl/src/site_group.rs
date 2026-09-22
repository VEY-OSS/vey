/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::str::FromStr;

use anyhow::anyhow;
use clap::{Arg, ArgMatches, Command, value_parser};

use vey_ctl::{CommandError, CommandResult};

use vey_proxy_proto::proc_capnp::proc_control;
use vey_proxy_proto::site_group_capnp::list_upstream_result;
use vey_proxy_proto::site_group_capnp::site_group_control;

use crate::common::parse_operation_result;

pub const COMMAND_LIST: &str = "site-upstream";
pub const COMMAND_SET_WEIGHT: &str = "set-site-upstream-weight";

const ARG_GROUP: &str = "group";
const ARG_SITE: &str = "site";
const ARG_ADDR: &str = "addr";
const ARG_WEIGHT: &str = "weight";

pub fn list_command() -> Command {
    Command::new(COMMAND_LIST)
        .about("List weighted upstream addresses for a site")
        .arg(Arg::new(ARG_GROUP).value_name("GROUP").required(true))
        .arg(Arg::new(ARG_SITE).value_name("SITE-ID").required(true))
}

pub fn set_weight_command() -> Command {
    Command::new(COMMAND_SET_WEIGHT)
        .about("Set the runtime weight of one site upstream address")
        .arg(Arg::new(ARG_GROUP).value_name("GROUP").required(true))
        .arg(Arg::new(ARG_SITE).value_name("SITE-ID").required(true))
        .arg(Arg::new(ARG_ADDR).value_name("IP:PORT").required(true))
        .arg(
            Arg::new(ARG_WEIGHT)
                .value_name("WEIGHT")
                .required(true)
                .value_parser(value_parser!(f64)),
        )
}

pub async fn list(client: &proc_control::Client, args: &ArgMatches) -> CommandResult<()> {
    let group = args.get_one::<String>(ARG_GROUP).unwrap();
    let site = args.get_one::<String>(ARG_SITE).unwrap();
    let site_group = super::proc::get_site_group(client, group).await?;
    list_upstream(&site_group, site).await
}

pub async fn set_weight(client: &proc_control::Client, args: &ArgMatches) -> CommandResult<()> {
    let group = args.get_one::<String>(ARG_GROUP).unwrap();
    let site = args.get_one::<String>(ARG_SITE).unwrap();
    let addr = args.get_one::<String>(ARG_ADDR).unwrap();
    let weight = *args.get_one::<f64>(ARG_WEIGHT).unwrap();
    SocketAddr::from_str(addr)
        .map_err(|e| CommandError::Cli(anyhow!("invalid upstream address {addr}: {e}")))?;
    if !weight.is_finite() || weight < 0.0 {
        return Err(CommandError::Cli(anyhow!(
            "weight must be a finite number >= 0"
        )));
    }
    let site_group = super::proc::get_site_group(client, group).await?;
    set_upstream_weight(&site_group, site, addr, weight).await
}

async fn list_upstream(client: &site_group_control::Client, site: &str) -> CommandResult<()> {
    let mut req = client.list_upstream_request();
    req.get().set_site_id(site);
    let rsp = req.send().promise.await?;
    let result = rsp.get()?.get_result()?;
    match result
        .which()
        .map_err(|e| anyhow!("invalid list upstream result: {e}"))?
    {
        list_upstream_result::Which::Peers(peers) => {
            println!("addr\tconfig_weight\tweight");
            for peer in peers? {
                let addr = peer
                    .get_addr()?
                    .to_str()
                    .map_err(|e| anyhow!("invalid upstream address: {e}"))?;
                println!(
                    "{addr}\t{}\t{}",
                    peer.get_config_weight(),
                    peer.get_weight()
                );
            }
            Ok(())
        }
        list_upstream_result::Which::Err(err) => {
            let e = err?;
            Err(CommandError::api_error(e.get_code(), e.get_reason()?))
        }
    }
}

async fn set_upstream_weight(
    client: &site_group_control::Client,
    site: &str,
    addr: &str,
    weight: f64,
) -> CommandResult<()> {
    let mut req = client.set_upstream_weight_request();
    req.get().set_site_id(site);
    req.get().set_addr(addr);
    req.get().set_weight(weight);
    let rsp = req.send().promise.await?;
    parse_operation_result(rsp.get()?.get_result()?)
}
