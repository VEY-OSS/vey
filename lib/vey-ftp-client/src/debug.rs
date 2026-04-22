/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicBool, Ordering};

use log::Level;

pub const FTP_DEBUG_LOG_LEVEL: Level = Level::Debug;
pub const FTP_DEBUG_LOG_CMD: &str = "_VEY_FTP_CMD";
pub const FTP_DEBUG_LOG_RSP: &str = "_VEY_FTP_RSP";

pub static IO_LOG_ENABLED: AtomicBool = AtomicBool::new(false);

#[macro_export]
macro_rules! log_cmd {
    ($cmd:expr) => {
        if $crate::debug::io_log_enabled() {
            log::log!(
                target: $crate::FTP_DEBUG_LOG_CMD,
                $crate::FTP_DEBUG_LOG_LEVEL,
                "{}", $cmd,
            );
        }
    };
}

#[macro_export]
macro_rules! log_rsp {
    ($rsp:expr) => {
        if $crate::debug::io_log_enabled() {
            log::log!(
                target: $crate::FTP_DEBUG_LOG_RSP,
                $crate::FTP_DEBUG_LOG_LEVEL,
                "{}", $rsp,
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
