/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod base;
#[cfg(feature = "geoip")]
pub use base::as_ip_network;
pub use base::{as_domain_name, as_host, as_ipaddr};
