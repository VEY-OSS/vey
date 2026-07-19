/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_io_ext::{UdpRelayRemoteRecv, UdpRelayRemoteSend};

mod stats;
mod task;

use crate::module::udp_connect::UdpConnectError;

pub(crate) use stats::{
    ArcUdpRelayTaskRemoteStats, UdpRelayRemoteWrapperStats, UdpRelayTaskRemoteStats,
};
pub(crate) use task::UdpRelayTaskConf;

pub(crate) type UdpRelaySetupResult = Result<
    (
        Box<dyn UdpRelayRemoteRecv + Unpin + Send + Sync>,
        Box<dyn UdpRelayRemoteSend + Unpin + Send + Sync>,
    ),
    UdpConnectError,
>;
