/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod stats;
pub(crate) use stats::{H2ForwardTaskAliveGuard, HttpForwardTaskAliveGuard, HttpGuardServerStats};

mod task;

mod server;
pub(super) use server::HttpGuardServer;

mod host;
pub(crate) use host::HttpHost;
