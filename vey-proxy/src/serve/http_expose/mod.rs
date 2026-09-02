/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

mod stats;
use stats::{HttpExposeServerStats, HttpForwardTaskAliveGuard, HttpUntrustedTaskAliveGuard};

mod task;

mod server;
pub(super) use server::HttpExposeServer;

mod host;
pub(crate) use host::HttpHost;
