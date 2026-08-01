/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! Common wrappers around [`crate::protocol`] request/response types, shared by
//! key-operation servers and the crypto backend.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use openssl::pkey::{PKey, Private};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::Instant;

use vey_histogram::HistogramRecorder;
use vey_std_ext::time::DurationExt;

use crate::protocol::{KeylessAction, KeylessErrorResponse, KeylessRequest, KeylessResponse};

use super::{KeyServerDurationRecorder, KeyServerRequestStats, KeyServerStats};

#[derive(Clone)]
pub(crate) struct RequestProcessContext {
    pub(crate) msg_id: u32,
    create_time: Instant,
    pub(crate) create_datetime: DateTime<Utc>,
    duration_recorder: Arc<HistogramRecorder<u64>>,
}

impl RequestProcessContext {
    fn new(msg_id: u32, duration_recorder: Arc<HistogramRecorder<u64>>) -> Self {
        RequestProcessContext {
            msg_id,
            create_time: Instant::now(),
            create_datetime: Utc::now(),
            duration_recorder,
        }
    }

    pub(super) fn record_duration_stats(&self) {
        let _ = self
            .duration_recorder
            .record(self.duration().as_nanos_u64());
    }

    pub(crate) fn duration(&self) -> Duration {
        self.create_time.elapsed()
    }
}

pub(crate) struct WrappedKeylessResponse {
    pub(super) inner: KeylessResponse,
    pub(super) ctx: RequestProcessContext,
}

impl WrappedKeylessResponse {
    pub(crate) fn new(inner: KeylessResponse, ctx: RequestProcessContext) -> Self {
        WrappedKeylessResponse { inner, ctx }
    }
}

pub(crate) struct WrappedKeylessRequest {
    pub(crate) inner: KeylessRequest,
    pub(crate) stats: Arc<KeyServerRequestStats>,
    pub(super) ctx: RequestProcessContext,
    err_rsp: Option<KeylessErrorResponse>,
    pub(super) server_sem_permit: Option<OwnedSemaphorePermit>,
}

impl WrappedKeylessRequest {
    pub(super) fn new(
        req: KeylessRequest,
        err_rsp: Option<KeylessErrorResponse>,
        server_stats: &Arc<KeyServerStats>,
        duration_recorder: &KeyServerDurationRecorder,
    ) -> Self {
        let (stats, duration_recorder) = match req.action {
            KeylessAction::Ping => (
                server_stats.ping_pong.clone(),
                duration_recorder.ping_pong.clone(),
            ),
            KeylessAction::RsaDecrypt(_) => (
                server_stats.rsa_decrypt.clone(),
                duration_recorder.rsa_decrypt.clone(),
            ),
            KeylessAction::RsaSign(_) => (
                server_stats.rsa_sign.clone(),
                duration_recorder.rsa_sign.clone(),
            ),
            KeylessAction::RsaPssSign(_) => (
                server_stats.rsa_pss_sign.clone(),
                duration_recorder.rsa_pss_sign.clone(),
            ),
            KeylessAction::EcdsaSign(_) => (
                server_stats.ecdsa_sign.clone(),
                duration_recorder.ecdsa_sign.clone(),
            ),
            KeylessAction::Ed25519Sign => (
                server_stats.ed25519_sign.clone(),
                duration_recorder.ed25519_sign.clone(),
            ),
            KeylessAction::NotSet => (server_stats.noop.clone(), duration_recorder.noop.clone()),
        };
        stats.add_total();
        stats.inc_alive();
        let ctx = RequestProcessContext::new(req.id, duration_recorder);
        WrappedKeylessRequest {
            inner: req,
            stats,
            ctx,
            err_rsp,
            server_sem_permit: None,
        }
    }

    pub(super) fn take_err_rsp(&mut self) -> Option<KeylessErrorResponse> {
        self.err_rsp.take()
    }

    pub(crate) fn process_by_openssl(&self, key: &PKey<Private>) -> KeylessResponse {
        match self.inner.process(key) {
            Ok(d) => {
                self.stats.add_passed();
                KeylessResponse::Data(d)
            }
            Err(e) => {
                self.stats.add_by_error_code(e.error_code());
                KeylessResponse::Error(e)
            }
        }
    }

    pub(crate) fn build_response(&self, rsp: KeylessResponse) -> WrappedKeylessResponse {
        WrappedKeylessResponse::new(rsp, self.ctx.clone())
    }
}

impl Drop for WrappedKeylessRequest {
    fn drop(&mut self) {
        self.stats.dec_alive();
    }
}
