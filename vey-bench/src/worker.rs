/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::sync::OnceLock;

use anyhow::anyhow;
use tokio::runtime::Handle;

use vey_runtime::unaided::{UnaidedRuntimeConfig, WorkersGuard};

static WORKER_HANDLERS: OnceLock<Vec<Handle>> = OnceLock::new();

pub fn spawn_workers(config: &UnaidedRuntimeConfig) -> anyhow::Result<WorkersGuard> {
    let mut handles = Vec::with_capacity(config.thread_number_total().get());
    let guard = config.start(|_, handle, _| handles.push(handle))?;
    WORKER_HANDLERS
        .set(handles)
        .map_err(|_| anyhow!("workers have already been spawned"))?;
    Ok(guard)
}

pub(super) fn select_handle(concurrency_index: usize) -> Option<Handle> {
    let handlers = WORKER_HANDLERS.get()?;
    match handlers.len() {
        0 => None,
        1 => Some(handlers[0].clone()),
        n => {
            let handle = unsafe { handlers.get_unchecked(concurrency_index % n) };
            Some(handle.clone())
        }
    }
}
