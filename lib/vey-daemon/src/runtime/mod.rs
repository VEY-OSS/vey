/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::OnceLock;

use log::warn;
use tokio::runtime::Handle;

pub mod config;
pub mod worker;

pub mod metrics;

static MAIN_HANDLE: OnceLock<Handle> = OnceLock::new();

pub fn main_handle() -> Option<&'static Handle> {
    MAIN_HANDLE.get()
}

pub fn set_main_handle() {
    let handle = Handle::current();
    metrics::add_tokio_stats(handle.metrics(), "main".into());
    if MAIN_HANDLE.set(handle).is_err() {
        warn!("main handle has already been set");
    }
}
