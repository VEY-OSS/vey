/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use slog::Logger;

use vey_slog_types::{
    LtDateTime, LtDuration, LtHttpMethod, LtHttpUri, LtIpAddr, LtUpstreamAddr, LtUserName, LtUuid,
};
use vey_types::net::UpstreamAddr;

use super::TaskEvent;
use crate::escape::EgressNotes;
use crate::module::http_forward::HttpForwardTaskNotes;
use crate::serve::ServerTaskNotes;

pub(crate) struct TaskLogForH2Forward<'a> {
    pub(crate) logger: &'a Logger,
    pub(crate) task_type: &'static str,
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) task_notes: &'a ServerTaskNotes,
    pub(crate) http_notes: &'a HttpForwardTaskNotes,
    pub(crate) egress_notes: &'a EgressNotes,
    pub(crate) client_rd_bytes: u64,
    pub(crate) client_wr_bytes: u64,
    pub(crate) remote_rd_bytes: u64,
    pub(crate) remote_wr_bytes: u64,
}

impl TaskLogForH2Forward<'_> {
    pub(crate) fn log_created(&self) {
        slog::info!(self.logger, "";
            "task_type" => self.task_type,
            "task_id" => LtUuid(&self.task_notes.id),
            "task_event" => TaskEvent::Created.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "upstream" => LtUpstreamAddr(self.upstream),
            "method" => LtHttpMethod(&self.http_notes.method),
            "uri" => LtHttpUri::new(&self.http_notes.uri, self.http_notes.uri_log_max_chars),
            "wait_time" => LtDuration(self.task_notes.wait_time),
        );
    }

    pub(crate) fn log(&self, err: &str) {
        slog::info!(self.logger, "{err}";
            "task_type" => self.task_type,
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
            "reuse_connection" => self.http_notes.reused_connection,
            "method" => LtHttpMethod(&self.http_notes.method),
            "uri" => LtHttpUri::new(&self.http_notes.uri, self.http_notes.uri_log_max_chars),
            "rsp_status" => self.http_notes.rsp_status,
            "origin_status" => self.http_notes.origin_status,
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "dur_req_send_hdr" => LtDuration(self.http_notes.dur_req_send_hdr),
            "dur_req_send_all" => LtDuration(self.http_notes.dur_req_send_all),
            "dur_rsp_recv_hdr" => LtDuration(self.http_notes.dur_rsp_recv_hdr),
            "dur_rsp_recv_all" => LtDuration(self.http_notes.dur_rsp_recv_all),
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
            "r_rd_bytes" => self.remote_rd_bytes,
            "r_wr_bytes" => self.remote_wr_bytes,
        );
    }
}
