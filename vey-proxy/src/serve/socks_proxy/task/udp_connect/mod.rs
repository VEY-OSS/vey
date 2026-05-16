/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{CommonTaskContext, SocksProxyServerStats};

mod task;
pub(super) use task::SocksProxyUdpConnectTask;

mod recv;
mod send;
mod stats;

use recv::Socks5UdpConnectClientRecv;
use send::Socks5UdpConnectClientSend;
use stats::UdpConnectTaskCltWrapperStats;
