/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::time::Duration;

use jiff::Timestamp;
use openssl::ssl::SslRef;
use tokio::time::Instant;

use vey_socket::BindAddr;
use vey_types::metrics::NodeName;
use vey_types::net::{AlpnProtocol, EgressInfo, UpstreamAddr};

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

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct RelayNotes {
    pub(crate) bind: Option<BindAddr>,
    pub(crate) local: Option<SocketAddr>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum EgressSocketType {
    Direct,
    Http,
    Socks5,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EgressNotes {
    pub(crate) escaper: NodeName,
    pub(crate) bind: BindAddr,
    pub(crate) tries: usize,
    pub(crate) expire: Option<Timestamp>,
    pub(crate) expire_at: Option<Instant>,
    pub(crate) egress: Option<EgressInfo>,
    pub(crate) socket_type: Option<EgressSocketType>,
    pub(crate) tcp: ConnectNotes,
    pub(crate) udp: ConnectNotes,
    pub(crate) udp_relay_v4: RelayNotes,
    pub(crate) udp_relay_v6: RelayNotes,
    pub(crate) final_addr: FinalAddressNotes,
    pub(crate) duration: Duration,
    pub(crate) override_peer: Option<UpstreamAddr>,
    pub(crate) selected_alpn: Option<AlpnProtocol>,
}

impl EgressNotes {
    pub(crate) fn reset(&mut self) {
        *self = Default::default();
    }

    pub(crate) fn set_expire(&mut self, datetime: Option<Timestamp>, instant: Option<Instant>) {
        self.expire = datetime;
        self.expire_at = instant;
    }

    pub(crate) fn is_expired(&self) -> bool {
        self.expire_at
            .is_some_and(|d| d.saturating_duration_since(Instant::now()).is_zero())
    }

    pub(crate) fn record_selected_alpn(&mut self, ssl: &SslRef) {
        self.selected_alpn = ssl
            .selected_alpn_protocol()
            .and_then(AlpnProtocol::from_selected);
    }

    pub(crate) fn tcp_connect_peer_addr(&self) -> Option<SocketAddr> {
        let socket_type = self.socket_type?;
        match socket_type {
            EgressSocketType::Direct => self.tcp.peer,
            EgressSocketType::Http => self.tcp.peer,
            EgressSocketType::Socks5 => self.tcp.peer,
        }
    }

    pub(crate) fn tcp_connect_local_addr(&self) -> Option<SocketAddr> {
        let socket_type = self.socket_type?;
        match socket_type {
            EgressSocketType::Direct => self.tcp.local,
            EgressSocketType::Http => self.tcp.local,
            EgressSocketType::Socks5 => self.tcp.local,
        }
    }

    pub(crate) fn udp_connect_peer_addr(&self) -> Option<SocketAddr> {
        let socket_type = self.socket_type?;
        match socket_type {
            EgressSocketType::Direct => self.udp.peer,
            EgressSocketType::Http => self.tcp.peer,
            EgressSocketType::Socks5 => self.udp.peer,
        }
    }

    pub(crate) fn udp_connect_local_addr(&self) -> Option<SocketAddr> {
        let socket_type = self.socket_type?;
        match socket_type {
            EgressSocketType::Direct => self.udp.local,
            EgressSocketType::Http => self.tcp.local,
            EgressSocketType::Socks5 => self.udp.local,
        }
    }
}
