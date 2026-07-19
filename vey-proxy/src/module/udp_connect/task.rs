/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;

use chrono::{DateTime, Utc};

use vey_io_ext::LimitedUdpRelayConfig;
use vey_socket::BindAddr;
use vey_types::metrics::NodeName;
use vey_types::net::{EgressInfo, UpstreamAddr};

use crate::escape::{EgressNotes, FinalAddressNotes};

pub(crate) struct UdpConnectTaskConf<'a> {
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) relay: LimitedUdpRelayConfig,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct UdpConnectTaskNotes {
    pub(crate) escaper: NodeName,
    pub(crate) bind: BindAddr,
    pub(crate) next: Option<SocketAddr>,
    pub(crate) local: Option<SocketAddr>,
    pub(crate) expire: Option<DateTime<Utc>>,
    pub(crate) egress: Option<EgressInfo>,
    pub(crate) final_addr: FinalAddressNotes,
    pub(crate) override_peer: Option<UpstreamAddr>,
}

impl UdpConnectTaskNotes {
    pub(crate) fn fill_from_underlying_tcp(&mut self, tcp: EgressNotes) {
        self.bind = tcp.bind;
        self.next = tcp.tcp.peer;
        self.local = tcp.tcp.local;
        self.expire = tcp.expire;
        self.egress = tcp.egress;
        self.final_addr = tcp.final_addr;
        self.override_peer = tcp.override_peer;
    }
}
