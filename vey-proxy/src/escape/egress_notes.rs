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
#[derive(Debug, Clone, Default)]
pub(crate) struct FinalAddressNotes {
    pub(crate) target_addr: Option<SocketAddr>,
    pub(crate) outgoing_addr: Option<SocketAddr>,
}

impl FinalAddressNotes {
    fn reset(&mut self) {
        self.target_addr = None;
        self.outgoing_addr = None;
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EgressNotes {
    pub(crate) escaper: NodeName,
    pub(crate) bind: BindAddr,
    pub(crate) next: Option<SocketAddr>,
    pub(crate) tries: usize,
    pub(crate) local: Option<SocketAddr>,
    pub(crate) expire: Option<DateTime<Utc>>,
    pub(crate) egress: Option<EgressInfo>,
    pub(crate) final_addr: FinalAddressNotes,
    pub(crate) duration: Duration,
    pub(crate) override_peer: Option<UpstreamAddr>,
}

impl EgressNotes {
    pub(crate) fn reset(&mut self) {
        self.escaper.clear();
        self.bind = BindAddr::None;
        self.next = None;
        self.tries = 0;
        self.local = None;
        self.expire = None;
        self.egress = None;
        self.final_addr.reset();
        self.duration = Duration::ZERO;
        self.override_peer = None;
    }
}
