/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cell::RefCell;
use std::sync::{Arc, OnceLock, mpsc};

use anyhow::anyhow;

use vey_cert_agent::Request;

mod runtime;
pub(crate) use runtime::{duration_recorder, duration_stats, get_config};

mod openssl;
use openssl::OpensslIssuer;

mod stats;
pub(crate) use stats::IssuerStats;

static ISSUER_STATS: OnceLock<Arc<IssuerStats>> = OnceLock::new();

thread_local! {
    static LOCAL_ISSUER: RefCell<Option<OpensslIssuer>> = const { RefCell::new(None) };
}

pub(crate) struct GeneratedData {
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

pub(crate) fn stats() -> Option<Arc<IssuerStats>> {
    ISSUER_STATS.get().cloned()
}

pub fn load() -> anyhow::Result<()> {
    if !runtime::publish() {
        return if ISSUER_STATS.get().is_some() {
            Ok(())
        } else {
            Err(anyhow!("no issuer config available"))
        };
    }
    let stats = issuer_stats();
    if vey_daemon::runtime::worker::worker_count() == 0 {
        install_local(&stats)
    } else {
        install_on_workers(&stats)
    }
}

fn issuer_stats() -> Arc<IssuerStats> {
    Arc::clone(ISSUER_STATS.get_or_init(|| Arc::new(IssuerStats::default())))
}

fn install_on_workers(stats: &Arc<IssuerStats>) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();
    let workers = vey_daemon::runtime::worker::foreach(|worker| {
        let stats = Arc::clone(stats);
        let tx = tx.clone();
        worker.handle.spawn(async move {
            let result = install_local(&stats);
            let _ = tx.send(result);
        });
        Ok::<(), anyhow::Error>(())
    })?;
    drop(tx);
    for _ in 0..workers {
        rx.recv()
            .map_err(|_| anyhow!("worker dropped issuer install"))??;
    }
    Ok(())
}

fn install_local(stats: &Arc<IssuerStats>) -> anyhow::Result<()> {
    let issuer = OpensslIssuer::new(stats)?;
    LOCAL_ISSUER.with(|slot| {
        *slot.borrow_mut() = Some(issuer);
    });
    Ok(())
}

pub(crate) fn sign(req: &Request) -> anyhow::Result<GeneratedData> {
    LOCAL_ISSUER.with(|slot| {
        let mut slot = slot.borrow_mut();
        let issuer = slot
            .as_mut()
            .ok_or_else(|| anyhow!("issuer is not running"))?;
        issuer.sign(req)
    })
}
