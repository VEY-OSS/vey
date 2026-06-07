/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::net::SocketAddr;

use chrono::{DateTime, Utc};

use vey_io_ext::LimitedUdpRelayConfig;
use vey_socket::BindAddr;
use vey_types::metrics::NodeName;
use vey_types::net::{EgressInfo, SocketBufferConfig, UpstreamAddr};

use crate::module::tcp_connect::TcpConnectTaskNotes;

pub(crate) struct UdpConnectTaskConf<'a> {
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) sock_buf: SocketBufferConfig,
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
    pub(crate) override_peer: Option<UpstreamAddr>,
}

impl UdpConnectTaskNotes {
    pub(crate) fn fill_from_underlying_tcp(&mut self, tcp: TcpConnectTaskNotes) {
        self.bind = tcp.bind;
        self.next = tcp.next;
        self.local = tcp.local;
        self.expire = tcp.expire;
        self.egress = tcp.egress;
        self.override_peer = tcp.override_peer;
    }
}
