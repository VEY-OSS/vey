/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::atomic::{AtomicBool, Ordering};

use log::Level;

pub const FTP_DEBUG_LOG_LEVEL: Level = Level::Debug;
pub const FTP_DEBUG_LOG_TARGET: &str = "";

pub static IO_LOG_ENABLED: AtomicBool = AtomicBool::new(false);

#[macro_export]
macro_rules! log_msg {
    ($s:literal, $($arg:tt)+) => (
        log::log!(target: $crate::FTP_DEBUG_LOG_TARGET, $crate::FTP_DEBUG_LOG_LEVEL, concat!(": ", $s), $($arg)+)
    )
}

#[macro_export]
macro_rules! log_cmd {
    ($cmd:expr) => {
        if $crate::debug::io_log_enabled() {
            log::log!(
                target: $crate::FTP_DEBUG_LOG_TARGET,
                $crate::FTP_DEBUG_LOG_LEVEL,
                "> {}", $cmd,
            );
        }
    };
}

#[macro_export]
macro_rules! log_rsp {
    ($rsp:expr) => {
        if $crate::debug::io_log_enabled() {
            log::log!(
                target: $crate::FTP_DEBUG_LOG_TARGET,
                $crate::FTP_DEBUG_LOG_LEVEL,
                "< {}", $rsp,
            );
        }
    };
}

pub fn enable_io_log() {
    IO_LOG_ENABLED.store(true, Ordering::Relaxed);
}

pub fn disable_io_log() {
    IO_LOG_ENABLED.store(false, Ordering::Relaxed);
}

pub(crate) fn io_log_enabled() -> bool {
    IO_LOG_ENABLED.load(Ordering::Relaxed)
}
