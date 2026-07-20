/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_io_ext::LimitedUdpRelayConfig;
use vey_types::net::UpstreamAddr;

pub(crate) struct UdpConnectTaskConf<'a> {
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) relay: LimitedUdpRelayConfig,
}
