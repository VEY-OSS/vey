/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use log::{Level, LevelFilter, Log, Metadata, Record};

use vey_ftp_client::{FTP_DEBUG_LOG_CMD, FTP_DEBUG_LOG_RSP};

pub(crate) struct SyncLogger {
    verbose_level: u8,
}

impl SyncLogger {
    pub(crate) fn new(verbose_level: u8) -> Self {
        if verbose_level > 0 {
            vey_ftp_client::enable_io_log();
        }
        SyncLogger { verbose_level }
    }

    pub(crate) fn into_global_logger(self) -> Result<(), log::SetLoggerError> {
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(LevelFilter::Debug);
        Ok(())
    }
}

impl Log for SyncLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        match metadata.target() {
            FTP_DEBUG_LOG_CMD | FTP_DEBUG_LOG_RSP => match metadata.level() {
                Level::Trace => false,
                Level::Debug => self.verbose_level > 0,
                _ => true,
            },
            _ => false,
        }
    }

    fn log(&self, record: &Record) {
        match record.target() {
            FTP_DEBUG_LOG_CMD => {
                eprintln!("> {}", record.args());
            }
            FTP_DEBUG_LOG_RSP => {
                eprintln!("< {}", record.args());
            }
            _ => {
                eprintln!("{}: {}", record.target(), record.args());
            }
        }
    }

    fn flush(&self) {}
}
