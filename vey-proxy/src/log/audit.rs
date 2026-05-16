/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use slog::Logger;

use vey_types::metrics::NodeName;

pub(super) fn get_logger(log_type: &'static str, auditor_name: &NodeName) -> Option<Logger> {
    let config = crate::config::log::get_audit_default_config();
    let logger_name = format!("la-{auditor_name}");
    let common_values = slog::o!(
        "daemon_name" => crate::opts::daemon_group(),
        "log_type" => log_type,
        "pid" => std::process::id(),
        "auditor_name" => auditor_name.clone(),
    );
    config.build_logger(logger_name, log_type, common_values)
}
