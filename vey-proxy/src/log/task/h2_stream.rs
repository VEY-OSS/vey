/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use h2::StreamId;
use slog::Logger;
use uuid::Uuid;

use vey_slog_types::{LtDateTime, LtDuration, LtH2StreamId, LtUserName, LtUuid};

use super::TaskEvent;
use crate::serve::ServerTaskNotes;

const TASK_TYPE: &str = "H2Stream";

pub(crate) struct TaskLogForH2Stream<'a> {
    pub(crate) logger: &'a Logger,
    pub(crate) task_notes: &'a ServerTaskNotes,
    pub(crate) connection_id: &'a Uuid,
    pub(crate) clt_stream_id: &'a StreamId,
}

impl TaskLogForH2Stream<'_> {
    pub(crate) fn log(
        &self,
        result: &str,
        next_task_type: Option<&str>,
        next_task_id: Option<&Uuid>,
        rsp_status: Option<u16>,
    ) {
        slog::info!(self.logger, "{result}";
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
            "clt_stream" => LtH2StreamId(self.clt_stream_id),
            "next_task_type" => next_task_type,
            "next_task_id" => next_task_id.map(LtUuid),
            "rsp_status" => rsp_status,
            "wait_time" => LtDuration(self.task_notes.wait_time),
            "total_time" => LtDuration(self.task_notes.time_elapsed()),
        );
    }
}
