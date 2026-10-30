/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use clap::{Arg, ArgAction, Command, ValueHint, value_parser};

use vey_daemon::opts::{DaemonArgs, DaemonArgsExt};

const GLOBAL_ARG_VERSION: &str = "version";
const GLOBAL_ARG_GROUP_NAME: &str = "group-name";
const GLOBAL_ARG_CONFIG_FILE: &str = "config-file";

static DAEMON_GROUP: OnceLock<String> = OnceLock::new();

#[derive(Debug)]
pub struct ProcArgs {
    pub daemon_config: DaemonArgs,
}

impl ProcArgs {
    fn new(name: String) -> Self {
        ProcArgs {
            daemon_config: DaemonArgs::new(name),
        }
    }

    pub fn program_name(&self) -> &str {
        &self.daemon_config.program_name
    }
}

pub(crate) fn daemon_group() -> &'static str {
    DAEMON_GROUP.get().map(|s| s.as_str()).unwrap_or_default()
}

fn build_cli_args() -> Command {
    Command::new(crate::build::PKG_NAME)
        .disable_version_flag(true)
        .append_daemon_args()
        .arg(
            Arg::new(GLOBAL_ARG_VERSION)
                .help("Show version")
                .num_args(0)
                .action(ArgAction::SetTrue)
                .short('V')
                .long(GLOBAL_ARG_VERSION),
        )
        .arg(
            Arg::new(GLOBAL_ARG_GROUP_NAME)
                .help("Instance group name")
                .num_args(1)
                .value_name("GROUP NAME")
                .short('G')
                .long(GLOBAL_ARG_GROUP_NAME),
        )
        .arg(
            Arg::new(GLOBAL_ARG_CONFIG_FILE)
                .help("Config file path")
                .num_args(1)
                .value_name("CONFIG FILE")
                .value_hint(ValueHint::FilePath)
                .value_parser(value_parser!(PathBuf))
                .required_unless_present_any([GLOBAL_ARG_VERSION])
                .short('c')
                .long("config-file"),
        )
}

pub fn parse_clap() -> anyhow::Result<Option<ProcArgs>> {
    let mut args_parser = build_cli_args();
    let args = args_parser.get_matches_mut();

    let bin_name = args_parser
        .get_bin_name()
        .unwrap_or_else(|| args_parser.get_name());

    let mut proc_args = ProcArgs::new(String::from(bin_name));
    proc_args.daemon_config.parse_clap(&args, bin_name)?;

    if args.get_flag(GLOBAL_ARG_VERSION) {
        crate::build::print_version(proc_args.daemon_config.verbose_level);
        return Ok(None);
    }

    if let Some(config_file) = args.get_one::<PathBuf>(GLOBAL_ARG_CONFIG_FILE) {
        vey_daemon::opts::validate_and_set_config_file(
            config_file,
            &[bin_name, crate::build::PKG_NAME, "g3iploc"],
        )
        .context(format!(
            "failed to load config file {}",
            config_file.display()
        ))?;
    } else {
        return Err(anyhow!("no config file given"));
    }

    if let Some(group_name) = args.get_one::<String>(GLOBAL_ARG_GROUP_NAME) {
        DAEMON_GROUP
            .set(group_name.clone())
            .map_err(|_| anyhow!("daemon group has already been set"))?;
    }

    Ok(Some(proc_args))
}
