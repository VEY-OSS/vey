/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

mod recv;
pub use recv::{LimitedUdpCopyRemoteRecv, UdpCopyRemoteRecv};

mod send;
pub use send::{LimitedUdpCopyRemoteSend, UdpCopyRemoteSend};

#[derive(Error, Debug)]
pub enum UdpCopyRemoteError {
    #[error("recv failed: {0:?}")]
    RecvFailed(io::Error),
    #[error("recv closed")]
    RecvClosed,
    #[error("send failed: {0:?}")]
    SendFailed(io::Error),
    #[error("invalid packet: {0}")]
    InvalidPacket(String),
    #[error("remote session closed")]
    RemoteSessionClosed,
    #[error("remote session error: {0:?}")]
    RemoteSessionError(io::Error),
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
}
