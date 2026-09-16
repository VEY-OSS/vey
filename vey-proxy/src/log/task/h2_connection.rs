/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use jiff::Timestamp;
use slog::Logger;
use uuid::Uuid;

use vey_slog_types::{LtDateTime, LtDuration, LtUserName, LtUuid};

use super::TaskEvent;
use crate::serve::ServerTaskNotes;

const TASK_TYPE: &str = "H2Connection";

pub(crate) struct TaskLogForH2Connection<'a> {
    pub(crate) logger: &'a Logger,
    pub(crate) task_notes: &'a ServerTaskNotes,
    pub(crate) connection_id: &'a Uuid,
    pub(crate) stream_total: u64,
    pub(crate) stream_alive: i32,
    pub(crate) first_stream_at: Option<&'a Timestamp>,
    pub(crate) client_rd_bytes: u64,
    pub(crate) client_wr_bytes: u64,
}

impl TaskLogForH2Connection<'_> {
    pub(crate) fn log_created(&self) {
        slog::info!(self.logger, "";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => LtUuid(self.connection_id),
            "task_event" => TaskEvent::Created.as_str(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "wait_time" => LtDuration(self.task_notes.wait_time),
        );
    }

    pub(crate) fn log_periodic(&self) {
        slog::info!(self.logger, "";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => LtUuid(self.connection_id),
            "task_event" => TaskEvent::Periodic.as_str(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "first_stream_at" => self.first_stream_at.map(LtDateTime),
            "stream_total" => self.stream_total,
            "stream_alive" => self.stream_alive,
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
        );
    }

    pub(crate) fn log(&self, err: &str) {
        slog::info!(self.logger, "{err}";
            "task_type" => TASK_TYPE,
            "task_id" => LtUuid(&self.task_notes.id),
            "connection_id" => LtUuid(self.connection_id),
            "task_event" => TaskEvent::Finished.as_str(),
            "start_at" => LtDateTime(&self.task_notes.start_at),
            "user" => self.task_notes.raw_user_name().map(LtUserName),
            "tenant" => self.task_notes.tenant_user_name().map(LtUserName),
            "site" => self.task_notes.site_id().map(|s| s.as_str()),
            "server_addr" => self.task_notes.server_addr(),
            "client_addr" => self.task_notes.client_addr(),
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
            "first_stream_at" => self.first_stream_at.map(LtDateTime),
            "stream_total" => self.stream_total,
            "stream_alive" => self.stream_alive,
            "c_rd_bytes" => self.client_rd_bytes,
            "c_wr_bytes" => self.client_wr_bytes,
        );
    }
}
