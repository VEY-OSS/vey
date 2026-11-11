/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::{Arc, OnceLock};

use anyhow::anyhow;
use tokio::runtime::Handle;

use vey_cert_agent::Request;

mod config;
pub(crate) use config::{duration_recorder, duration_stats, get_config, load};

mod openssl;
use openssl::OpensslIssuer;

mod stats;
pub(crate) use stats::IssuerStats;

static REQUEST_SENDER: OnceLock<kanal::AsyncSender<CertRequest>> = OnceLock::new();
static ISSUER_STATS: OnceLock<Arc<IssuerStats>> = OnceLock::new();

pub(crate) struct GeneratedData {
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

pub(crate) struct IssuedCert {
    pub(crate) user_req: Request,
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

pub(crate) enum GenerateError {
    Queue(anyhow::Error),
    Failed,
    Dropped,
}

pub(super) struct CertRequest {
    user_req: Request,
    reply: tokio::sync::oneshot::Sender<Result<IssuedCert, ()>>,
}

pub(crate) fn stats() -> Option<Arc<IssuerStats>> {
    ISSUER_STATS.get().cloned()
}

pub(crate) async fn generate(user_req: Request) -> Result<IssuedCert, GenerateError> {
    let (reply, response) = tokio::sync::oneshot::channel();
    REQUEST_SENDER
        .get()
        .expect("issuer is not running")
        .send(CertRequest { user_req, reply })
        .await
        .map_err(|e| GenerateError::Queue(anyhow!("failed to send request to issuer: {e}")))?;
    match response.await {
        Ok(Ok(issued)) => Ok(issued),
        Ok(Err(())) => Err(GenerateError::Failed),
        Err(_) => Err(GenerateError::Dropped),
    }
}

pub(crate) fn spawn_workers() -> anyhow::Result<()> {
    if get_config().is_none() {
        return Err(anyhow!("no issuer config available"));
    }
    let issuer_stats = Arc::new(IssuerStats::default());
    let (req_sender, req_receiver) = kanal::bounded_async::<CertRequest>(1024);

    let workers = vey_daemon::runtime::worker::foreach(|h| {
        let issuer = OpensslIssuer::new(&issuer_stats)
            .map_err(|e| anyhow!("failed to build issuer for worker {}: {e}", h.id))?;
        issuer.spawn(&h.handle, h.id, req_receiver.clone());
        Ok::<(), anyhow::Error>(())
    })?;
    if workers < 1 {
        let issuer = OpensslIssuer::new(&issuer_stats)
            .map_err(|e| anyhow!("failed to build issuer for main runtime: {e}"))?;
        issuer.spawn(&Handle::current(), 0, req_receiver);
    }

    REQUEST_SENDER
        .set(req_sender)
        .map_err(|_| anyhow!("issuer is already running"))?;
    ISSUER_STATS
        .set(issuer_stats)
        .map_err(|_| anyhow!("issuer is already running"))?;
    Ok(())
}
