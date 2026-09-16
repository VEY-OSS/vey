/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use h2::StreamId;
use slog::Logger;
use uuid::Uuid;

use vey_slog_types::{
    LtDateTime, LtDuration, LtH2StreamId, LtHttpUri, LtHttpVersion, LtIpAddr, LtUpstreamAddr,
    LtUserName, LtUuid,
};
use vey_types::net::UpstreamAddr;

use super::TaskEvent;
use crate::escape::EgressNotes;
use crate::module::websocket::WebSocketTaskNotes;
use crate::serve::{ServerTaskError, ServerTaskNotes};

const TASK_TYPE: &str = "Websocket";

pub(crate) struct TaskLogForWebSocket<'a> {
    pub(crate) logger: &'a Logger,
    pub(crate) upstream: &'a UpstreamAddr,
    pub(crate) task_notes: &'a ServerTaskNotes,
    pub(crate) ws_notes: &'a WebSocketTaskNotes,
    pub(crate) egress_notes: &'a EgressNotes,
    pub(crate) client_rd_bytes: u64,
    pub(crate) client_wr_bytes: u64,
    pub(crate) remote_rd_bytes: u64,
    pub(crate) remote_wr_bytes: u64,
    pub(crate) clt_stream_id: Option<&'a StreamId>,
    pub(crate) ups_stream_id: Option<&'a StreamId>,
    pub(crate) connection_id: Option<&'a Uuid>,
}

impl TaskLogForWebSocket<'_> {
    fn skip_log(&self) -> bool {
        self.task_notes
            .user_ctx()
            .is_some_and(|user_ctx| user_ctx.skip_log())
    }

    pub(crate) fn log_created(&self) {
        if self.skip_log() {
            return;
        }

        slog::info!(self.logger, "";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => self.connection_id.map(LtUuid),
            "task_event" => TaskEvent::Created.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "clt_stream" => self.clt_stream_id.map(LtH2StreamId),
            "ups_stream" => self.ups_stream_id.map(LtH2StreamId),
            "upstream" => LtUpstreamAddr(self.upstream),
            "version" => LtHttpVersion(self.ws_notes.version),
            "uri" => LtHttpUri::new(&self.ws_notes.uri, self.ws_notes.uri_log_max_chars),
            "wait_time" => LtDuration(self.task_notes.wait_time),
        )
    }

    pub(crate) fn log_connected(&self) {
        if self.skip_log() {
            return;
        }

        slog::info!(self.logger, "";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => self.connection_id.map(LtUuid),
            "task_event" => TaskEvent::Connected.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "clt_stream" => self.clt_stream_id.map(LtH2StreamId),
            "ups_stream" => self.ups_stream_id.map(LtH2StreamId),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "version" => LtHttpVersion(self.ws_notes.version),
            "uri" => LtHttpUri::new(&self.ws_notes.uri, self.ws_notes.uri_log_max_chars),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
        )
    }

    pub(crate) fn log_periodic(&self) {
        if self.skip_log() {
            return;
        }

        slog::info!(self.logger, "";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => self.connection_id.map(LtUuid),
            "task_event" => TaskEvent::Periodic.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "clt_stream" => self.clt_stream_id.map(LtH2StreamId),
            "ups_stream" => self.ups_stream_id.map(LtH2StreamId),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "version" => LtHttpVersion(self.ws_notes.version),
            "uri" => LtHttpUri::new(&self.ws_notes.uri, self.ws_notes.uri_log_max_chars),
            "rsp_status" => self.ws_notes.rsp_status,
            "origin_status" => self.ws_notes.origin_status,
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
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => self.connection_id.map(LtUuid),
            "task_event" => task_event.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "clt_stream" => self.clt_stream_id.map(LtH2StreamId),
            "ups_stream" => self.ups_stream_id.map(LtH2StreamId),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "version" => LtHttpVersion(self.ws_notes.version),
            "uri" => LtHttpUri::new(&self.ws_notes.uri, self.ws_notes.uri_log_max_chars),
            "rsp_status" => self.ws_notes.rsp_status,
            "origin_status" => self.ws_notes.origin_status,
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

    pub(crate) fn log(&self, e: &ServerTaskError) {
        if self.skip_log() {
            return;
        }

        slog::info!(self.logger, "{}", e;
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => self.connection_id.map(LtUuid),
            "task_event" => TaskEvent::Finished.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "clt_stream" => self.clt_stream_id.map(LtH2StreamId),
            "ups_stream" => self.ups_stream_id.map(LtH2StreamId),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "reason" => e.brief(),
            "version" => LtHttpVersion(self.ws_notes.version),
            "uri" => LtHttpUri::new(&self.ws_notes.uri, self.ws_notes.uri_log_max_chars),
            "rsp_status" => self.ws_notes.rsp_status,
            "origin_status" => self.ws_notes.origin_status,
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
            "r_rd_bytes" => self.remote_rd_bytes,
            "r_wr_bytes" => self.remote_wr_bytes,
        )
    }

    pub(crate) fn log_h2(&self, err: &str) {
        if self.skip_log() {
            return;
        }

        slog::info!(self.logger, "{err}";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => self.connection_id.map(LtUuid),
            "task_event" => TaskEvent::Finished.as_str(),
            "stage" => self.task_notes.stage.brief(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "clt_stream" => self.clt_stream_id.map(LtH2StreamId),
            "ups_stream" => self.ups_stream_id.map(LtH2StreamId),
            "upstream" => LtUpstreamAddr(self.upstream),
            "escaper" => self.egress_notes.escaper.as_str(),
            "next_bind_ip" => self.egress_notes.bind.ip().map(LtIpAddr),
            "next_bound_addr" => self.egress_notes.tcp.local,
            "next_peer_addr" => self.egress_notes.tcp.peer,
            "next_expire" => self.egress_notes.expire.as_ref().map(LtDateTime),
            "tcp_connect_tries" => self.egress_notes.tries,
            "tcp_connect_spend" => LtDuration(self.egress_notes.duration),
            "version" => LtHttpVersion(self.ws_notes.version),
            "uri" => LtHttpUri::new(&self.ws_notes.uri, self.ws_notes.uri_log_max_chars),
            "rsp_status" => self.ws_notes.rsp_status,
            "origin_status" => self.ws_notes.origin_status,
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "ready_time" => LtDuration(self.task_notes.ready_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
            "r_rd_bytes" => self.remote_rd_bytes,
            "r_wr_bytes" => self.remote_wr_bytes,
        );
    }
}
