/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod daemon;
pub use daemon::{DaemonArgs, DaemonArgsExt};

#[cfg(unix)]
mod control;
#[cfg(unix)]
pub use control::{control_dir, validate_and_set_control_dir};

mod config;
pub use config::{config_dir, config_file, config_file_extension, validate_and_set_config_file};
