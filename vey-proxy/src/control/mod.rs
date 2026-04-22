/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::sync::{Mutex, RwLock};

mod bridge;

mod quit;
pub use quit::QuitActor;

mod upgrade;
pub use upgrade::UpgradeActor;

mod local;
pub use local::{DaemonController, UniqueController};

pub mod capnp;

static IO_MUTEX: RwLock<Option<Mutex<()>>> = RwLock::const_new(Some(Mutex::const_new(())));

pub(crate) async fn run_protected_io<F: Future>(future: F) -> Option<F::Output> {
    let outer = IO_MUTEX.read().await;
    if let Some(inner) = &*outer {
        // io tasks that should avoid corrupt at exit should hold this lock
        let _guard = inner.lock().await;
        Some(future.await)
    } else {
        None
    }
}

pub(crate) async fn disable_protected_io() {
    let mut outer = IO_MUTEX.write().await;
    if let Some(inner) = outer.take() {
        // wait all inner lock finish
        let _ = inner.lock().await;
    }
}
