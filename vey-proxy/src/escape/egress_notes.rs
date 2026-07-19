/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::time::Duration;

use chrono::{DateTime, Utc};

use vey_socket::BindAddr;
use vey_types::metrics::NodeName;
use vey_types::net::{EgressInfo, UpstreamAddr};

/// This contains the final chained info about the client request
#[derive(Debug, Clone, Default, Copy)]
pub(crate) struct FinalAddressNotes {
    pub(crate) target_addr: Option<SocketAddr>,
    pub(crate) outgoing_addr: Option<SocketAddr>,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ConnectNotes {
    pub(crate) peer: Option<SocketAddr>,
    pub(crate) local: Option<SocketAddr>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EgressSocketType {
    Tcp,
    Socks5,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EgressNotes {
    pub(crate) escaper: NodeName,
    pub(crate) bind: BindAddr,
    pub(crate) tries: usize,
    pub(crate) expire: Option<DateTime<Utc>>,
    pub(crate) egress: Option<EgressInfo>,
    pub(crate) socket_type: Option<EgressSocketType>,
    pub(crate) tcp: ConnectNotes,
    pub(crate) final_addr: FinalAddressNotes,
    pub(crate) duration: Duration,
    pub(crate) override_peer: Option<UpstreamAddr>,
}

impl EgressNotes {
    pub(crate) fn reset(&mut self) {
        *self = Default::default();
    }

    pub(crate) fn tcp_connect_peer_addr(&self) -> Option<SocketAddr> {
        let socket_type = self.socket_type?;
        match socket_type {
            EgressSocketType::Tcp => self.tcp.peer,
            EgressSocketType::Socks5 => self.tcp.peer,
        }
    }

    pub(crate) fn tcp_connect_local_addr(&self) -> Option<SocketAddr> {
        let socket_type = self.socket_type?;
        match socket_type {
            EgressSocketType::Tcp => self.tcp.local,
            EgressSocketType::Socks5 => self.tcp.local,
        }
    }
}
