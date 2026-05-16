/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod stats;
pub use stats::{ListenAliveGuard, ListenSnapshot, ListenStats};

mod tcp;
pub use tcp::{AcceptTcpServer, ListenTcpRuntime};

mod udp;
pub use udp::{
    AcceptUdpServer, AcceptedUdpPacketReceiver, AcceptedUdpPacketSender, ListenUdpRuntime,
    ReceiveUdpRuntime, ReceiveUdpServer,
};

#[cfg_attr(feature = "quic", path = "quic.rs")]
#[cfg_attr(not(feature = "quic"), path = "no_quic.rs")]
mod quic;
pub use quic::{AcceptQuicServer, ListenQuicConf, ListenQuicRuntime};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;
