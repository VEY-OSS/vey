/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

mod recv;
pub use recv::{LimitedUdpCopyClientRecv, UdpCopyClientRecv};

mod send;
pub use send::{LimitedUdpCopyClientSend, UdpCopyClientSend};

#[derive(Error, Debug)]
pub enum UdpCopyClientError {
    #[error("recv failed: {0:?}")]
    RecvFailed(io::Error),
    #[error("send failed: {0:?}")]
    SendFailed(io::Error),
    #[error("invalid packet: {0}")]
    InvalidPacket(String),
    #[error("mismatched client address")]
    MismatchedClientAddress,
    #[error("vary upstream")]
    VaryUpstream,
    #[error("forbidden client address")]
    ForbiddenClientAddress,
}
