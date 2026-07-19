/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use vey_types::net::{SocketBufferConfig, UpstreamAddr};

pub(crate) struct UdpRelayTaskConf<'a> {
    pub(crate) initial_peer: &'a UpstreamAddr,
    pub(crate) sock_buf: SocketBufferConfig,
}
