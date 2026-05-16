/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;
mod server;
mod stats;
mod task;

pub(super) use server::UdpStreamServer;
pub(crate) use stats::{
    UdpStreamServerAliveTaskGuard, UdpStreamServerStats, UdpStreamTaskCltWrapperStats,
};
