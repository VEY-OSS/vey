/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use slog::Logger;

use vey_slog_types::{LtDateTime, LtDuration, LtIpAddr, LtUpstreamAddr, LtUserName, LtUuid};
use vey_types::net::UpstreamAddr;

use super::TaskEvent;
use crate::escape::EgressNotes;
use crate::serve::{ServerTaskError, ServerTaskNotes};

pub(crate) struct TaskLogForTcpConnect<'a> {
    pub(crate) logger: &'a Logger,
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) task_notes: &'a ServerTaskNotes,
    pub(crate) egress_notes: &'a EgressNotes,
    pub(crate) client_rd_bytes: u64,
    pub(crate) client_wr_bytes: u64,
    pub(crate) remote_rd_bytes: u64,
    pub(crate) remote_wr_bytes: u64,
}

impl TaskLogForTcpConnect<'_> {
    pub(crate) fn log_created(&self) {
        if let Some(user_ctx) = self.task_notes.user_ctx()
            && user_ctx.skip_log()
        {
            return;
        }

        slog::info!(self.logger, "";
            "task_type" => "TcpConnect",
            "task_id" => LtUuid(&self.task_notes.id),
            "task_event" => TaskEvent::Created.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "upstream" => LtUpstreamAddr(self.upstream),
            "wait_time" => LtDuration(self.task_notes.wait_time),
        )
    }

    pub(crate) fn log_connected(&self) {
        if let Some(user_ctx) = self.task_notes.user_ctx()
            && user_ctx.skip_log()
        {
            return;
        }

        slog::info!(self.logger, "";
            "task_type" => "TcpConnect",
            "task_id" => LtUuid(&self.task_notes.id),
            "task_event" => TaskEvent::Connected.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
        )
    }

    pub(crate) fn log_periodic(&self) {
        if let Some(user_ctx) = self.task_notes.user_ctx()
            && user_ctx.skip_log()
        {
            return;
        }

        slog::info!(self.logger, "";
            "task_type" => "TcpConnect",
            "task_id" => LtUuid(&self.task_notes.id),
            "task_event" => TaskEvent::Periodic.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
            "r_rd_bytes" => self.remote_rd_bytes,
            "r_wr_bytes" => self.remote_wr_bytes,
        )
    }

    fn log_partial_shutdown(&self, task_event: TaskEvent) {
        slog::info!(self.logger, "";
            "task_type" => "TcpConnect",
            "task_id" => LtUuid(&self.task_notes.id),
            "task_event" => task_event.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
            "r_rd_bytes" => self.remote_rd_bytes,
            "r_wr_bytes" => self.remote_wr_bytes,
        )
    }

    pub(crate) fn log_client_shutdown(&self) {
        self.log_partial_shutdown(TaskEvent::ClientShutdown);
    }

    pub(crate) fn log_upstream_shutdown(&self) {
        self.log_partial_shutdown(TaskEvent::UpstreamShutdown);
    }

    pub(crate) fn log(&self, e: ServerTaskError) {
        if let Some(user_ctx) = self.task_notes.user_ctx()
            && user_ctx.skip_log()
        {
            return;
        }

        slog::info!(self.logger, "{}", e;
            "task_type" => "TcpConnect",
            "task_id" => LtUuid(&self.task_notes.id),
            "task_event" => TaskEvent::Finished.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "reason" => e.brief(),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
            "r_rd_bytes" => self.remote_rd_bytes,
            "r_wr_bytes" => self.remote_wr_bytes,
        )
    }
}
