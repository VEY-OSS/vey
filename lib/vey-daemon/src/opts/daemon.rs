/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;

#[cfg(unix)]
use anyhow::Context;
use clap::{Arg, ArgAction, ArgMatches, Command, ValueHint, value_parser};

const ARGS_VERBOSE: &str = "verbose";
const ARGS_DAEMON: &str = "daemon";
const ARGS_SYSTEMD: &str = "systemd";
const ARGS_MONITORED: &str = "monitored";
const ARGS_PID_FILE: &str = "pid-file";
const ARGS_CONTROL_DIR: &str = "control-dir";
const ARGS_TEST_CONFIG: &str = "test-config";
const ARGS_PANIC_QUIT: &str = "panic-quit";

pub trait DaemonArgsExt {
    fn append_daemon_args(self) -> Self;
}

#[derive(Debug)]
pub struct DaemonArgs {
    pub(crate) with_systemd: bool,
    pub(crate) daemon_mode: bool,
    pub(crate) monitored: bool,
    pub verbose_level: u8,
    pub program_name: String,
    pub pid_file: Option<PathBuf>,
    pub test_config: bool,
    pub(crate) panic_quit: bool,
}

impl DaemonArgs {
    pub fn new(process_name: String) -> Self {
        DaemonArgs {
            with_systemd: false,
            daemon_mode: false,
            monitored: false,
            verbose_level: 0,
            program_name: process_name,
            pid_file: None,
            test_config: false,
            panic_quit: false,
        }
    }

    fn set_with_systemd(&mut self) {
        self.with_systemd = cfg!(target_os = "linux");
    }

    fn enable_daemon_mode(&mut self) {
        self.daemon_mode = cfg!(all(unix, not(target_os = "macos")));
    }

    pub fn need_daemon_controller(&self) -> bool {
        self.daemon_mode || self.with_systemd || self.monitored
    }

    pub fn parse_clap(&mut self, args: &ArgMatches, app_name: &str) -> anyhow::Result<()> {
        if let Some(verbose_level) = args.get_one::<u8>(ARGS_VERBOSE) {
            self.verbose_level = *verbose_level;
        }
        if args.get_flag(ARGS_TEST_CONFIG) {
            self.test_config = true;
        }
        if args.get_flag(ARGS_DAEMON) {
            self.enable_daemon_mode();
        }
        if args.get_flag(ARGS_SYSTEMD) {
            self.set_with_systemd();
        }
        if args.get_flag(ARGS_MONITORED) {
            self.monitored = true;
        }
        if let Some(pid_file) = args.get_one::<PathBuf>(ARGS_PID_FILE) {
            self.pid_file = Some(pid_file.to_path_buf());
        }
        if args.get_flag(ARGS_PANIC_QUIT) {
            self.panic_quit = true;
        }
        self.set_control_dir(app_name, args)
    }

    #[cfg(unix)]
    fn set_control_dir(&mut self, app_name: &str, args: &ArgMatches) -> anyhow::Result<()> {
        super::validate_and_set_control_dir(
            app_name,
            args.get_one::<PathBuf>(ARGS_CONTROL_DIR)
                .map(PathBuf::as_path),
        )
        .context("invalid control dir")
    }

    #[cfg(not(unix))]
    fn set_control_dir(&mut self, _app_name: &str, _args: &ArgMatches) -> anyhow::Result<()> {
        Ok(())
    }
}

impl DaemonArgsExt for Command {
    fn append_daemon_args(self) -> Self {
        self.arg(
            Arg::new(ARGS_VERBOSE)
                .help("Show verbose output")
                .num_args(0)
                .action(ArgAction::Count)
                .short('v')
                .long(ARGS_VERBOSE),
        )
        .arg(
            Arg::new(ARGS_DAEMON)
                .help("Run in daemon mode")
                .action(ArgAction::SetTrue)
                .requires(ARGS_PID_FILE)
                .short('d')
                .long(ARGS_DAEMON),
        )
        .arg(
            Arg::new(ARGS_SYSTEMD)
                .help("Run with systemd")
                .action(ArgAction::SetTrue)
                .short('s')
                .long(ARGS_SYSTEMD),
        )
        .arg(
            Arg::new(ARGS_MONITORED)
                .help("Run in monitored mode")
                .action(ArgAction::SetTrue)
                .short('m')
                .long(ARGS_MONITORED),
        )
        .arg(
            Arg::new(ARGS_PID_FILE)
                .help("Pid file for daemon mode")
                .num_args(1)
                .value_name("PID FILE")
                .value_hint(ValueHint::FilePath)
                .value_parser(value_parser!(PathBuf))
                .short('p')
                .long(ARGS_PID_FILE),
        )
        .arg(
            Arg::new(ARGS_CONTROL_DIR)
                .help("Control socket directory")
                .num_args(1)
                .value_name("CONTROL DIR")
                .value_hint(ValueHint::DirPath)
                .value_parser(value_parser!(PathBuf))
                .short('C')
                .long(ARGS_CONTROL_DIR),
        )
        .arg(
            Arg::new(ARGS_TEST_CONFIG)
                .help("Test the format of config file and exit")
                .action(ArgAction::SetTrue)
                .short('t')
                .long(ARGS_TEST_CONFIG),
        )
        .arg(
            Arg::new(ARGS_PANIC_QUIT)
                .help("Quit the process if panic")
                .action(ArgAction::SetTrue)
                .long(ARGS_PANIC_QUIT),
        )
    }
}
