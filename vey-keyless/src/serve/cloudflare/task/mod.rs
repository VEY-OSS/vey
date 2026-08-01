/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use chrono::{DateTime, Utc};
use slog::Logger;
use tokio::io::AsyncRead;
use tokio::sync::{Semaphore, broadcast};
use uuid::Uuid;

use vey_daemon::server::ClientConnectionInfo;
use vey_slog_types::{LtDateTime, LtUuid};

use crate::config::server::cloudflare::CloudflareServerConfig;
use crate::protocol::KeylessRequest;
use crate::serve::{
    KeyServerAliveTaskGuard, KeyServerDurationRecorder, KeyServerStats, ServerReloadCommand,
    ServerTaskError, WrappedKeylessRequest,
};

mod multiplex;
mod simplex;

pub(crate) struct KeylessTaskContext {
    pub(crate) server_config: Arc<CloudflareServerConfig>,
    pub(crate) server_stats: Arc<KeyServerStats>,
    pub(crate) duration_recorder: KeyServerDurationRecorder,
    pub(crate) cc_info: ClientConnectionInfo,
    pub(crate) task_logger: Option<Logger>,
    pub(crate) request_logger: Option<Logger>,
    pub(crate) reload_notifier: broadcast::Receiver<ServerReloadCommand>,
    pub(crate) concurrency_limit: Option<Arc<Semaphore>>,
}

pub(crate) struct KeylessTask {
    id: Uuid,
    ctx: KeylessTaskContext,
    started: DateTime<Utc>,
    buf: Vec<u8>,
    #[cfg(feature = "openssl-async-job")]
    allow_openssl_async_job: bool,
    allow_dispatch: bool,
    _alive_guard: KeyServerAliveTaskGuard,
}

impl KeylessTask {
    pub(crate) fn new(ctx: KeylessTaskContext) -> Self {
        let alive_guard = ctx.server_stats.add_task();
        let started = Utc::now();
        KeylessTask {
            id: vey_daemon::server::task::generate_uuid(&started),
            ctx,
            started,
            buf: Vec::with_capacity(KeylessRequest::CLOUDFLARE_READ_BUF_CAPACITY),
            #[cfg(feature = "openssl-async-job")]
            allow_openssl_async_job: false,
            allow_dispatch: false,
            _alive_guard: alive_guard,
        }
    }

    pub(crate) fn set_allow_dispatch(&mut self) {
        self.allow_dispatch = true;
    }

    #[cfg(feature = "openssl-async-job")]
    pub(crate) fn set_allow_openssl_async_job(&mut self) {
        self.allow_openssl_async_job = true;
    }

    async fn timed_read_request<R>(
        &mut self,
        reader: &mut R,
        msg_count: usize,
    ) -> Result<WrappedKeylessRequest, ServerTaskError>
    where
        R: AsyncRead + Unpin,
    {
        match tokio::time::timeout(
            self.ctx.server_config.request_read_timeout,
            KeylessRequest::read_cloudflare(reader, &mut self.buf, msg_count),
        )
        .await
        {
            Ok(Ok(mut req)) => {
                let err_rsp = req.verify_cloudflare_opcode().err();
                Ok(WrappedKeylessRequest::new(
                    req,
                    err_rsp,
                    &self.ctx.server_stats,
                    &self.ctx.duration_recorder,
                ))
            }
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(ServerTaskError::ReadTimeout),
        }
    }

    fn log_task_err(&self, e: ServerTaskError) {
        if e.ignore_log() {
            return;
        }
        if let Some(logger) = &self.ctx.task_logger {
            slog::info!(logger, "{}", e;
                "task_id" => LtUuid(&self.id),
                "start_at" => LtDateTime(&self.started),
                "server_addr" => self.ctx.cc_info.server_addr(),
                "client_addr" => self.ctx.cc_info.client_addr(),
            );
        }
    }

    fn log_task_ok(&self) {
        self.log_task_err(ServerTaskError::NoError)
    }
}
