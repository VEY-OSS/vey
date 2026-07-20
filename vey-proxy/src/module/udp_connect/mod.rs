/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_io_ext::{UdpCopyRemoteRecv, UdpCopyRemoteSend};

mod error;
mod stats;
mod task;

pub(crate) use error::UdpConnectError;
pub(crate) use stats::UdpConnectRemoteWrapperStats;
pub(crate) use task::UdpConnectTaskConf;

pub(crate) type UdpConnection = (
    Box<dyn UdpCopyRemoteRecv + Unpin + Send + Sync>,
    Box<dyn UdpCopyRemoteSend + Unpin + Send + Sync>,
);
pub(crate) type UdpConnectResult = Result<UdpConnection, UdpConnectError>;
