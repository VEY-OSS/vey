/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use slog::Logger;
use uuid::Uuid;

use vey_slog_types::{LtDateTime, LtDuration, LtIpAddr, LtUpstreamAddr, LtUuid};
use vey_types::net::UpstreamAddr;

use crate::escape::EgressNotes;
use crate::module::tcp_connect::UnderlyingTcpConnectError;

pub(crate) struct EscapeLogForTcpConnect<'a> {
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) egress_notes: &'a EgressNotes,
    pub(crate) task_id: &'a Uuid,
}

impl EscapeLogForTcpConnect<'_> {
    pub(crate) fn log(&self, logger: &Logger, e: &UnderlyingTcpConnectError) {
        slog::info!(logger, "{}", e;
            "escape_type" => "TcpConnect",
            "task_id" => LtUuid(self.task_id),
            "upstream" => LtUpstreamAddr(self.upstream),
            "override_peer" => self.egress_notes.override_peer.as_ref().map(LtUpstreamAddr),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "reason" => e.brief(),
        )
    }
}
