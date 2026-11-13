/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cell::RefCell;
use std::sync::mpsc;

use anyhow::anyhow;

use vey_cert_agent::Request;

use crate::config::issuer::OpensslIssuerConfig;

mod openssl;
use openssl::OpensslIssuer;

mod stats;
pub(crate) use stats::{issuer_duration_recorder, issuer_stats};

thread_local! {
    static LOCAL_ISSUER: RefCell<Option<OpensslIssuer>> = const { RefCell::new(None) };
}

pub(crate) struct GeneratedData {
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

pub fn load() -> anyhow::Result<()> {
    let Some(config) = crate::config::issuer::get_config() else {
        return Err(anyhow!("no issuer configuration found"));
    };
    stats::load_duration_stats(&config);
    if vey_daemon::runtime::worker::worker_count() == 0 {
        install_local(config)
    } else {
        install_on_workers(config)
    }
}

fn install_on_workers(config: OpensslIssuerConfig) -> anyhow::Result<()> {
    let (tx, rx) = mpsc::channel();
    let workers = vey_daemon::runtime::worker::foreach(|worker| {
        let config = config.clone();
        let tx = tx.clone();
        worker.handle.spawn(async move {
            let result = install_local(config);
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

fn install_local(config: OpensslIssuerConfig) -> anyhow::Result<()> {
    let issuer = OpensslIssuer::new(config)?;
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
