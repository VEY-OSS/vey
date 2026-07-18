/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::io::{AsyncRead, AsyncWrite};

mod error;
mod stats;
mod task;

pub(crate) use error::{TcpConnectError, UnderlyingTcpConnectError};
pub(crate) use stats::TcpConnectRemoteWrapperStats;
pub(crate) use task::{TcpConnectTaskConf, TlsConnectTaskConf};

pub(crate) type TcpConnection = (
    Box<dyn AsyncRead + Unpin + Send + Sync>,
    Box<dyn AsyncWrite + Unpin + Send + Sync>,
);
pub(crate) type TcpConnectResult = Result<TcpConnection, TcpConnectError>;
