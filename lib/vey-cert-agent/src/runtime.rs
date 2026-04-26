/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Mutex;
use std::thread::JoinHandle;

use tokio::runtime::{Handle, RuntimeMetrics};
use tokio::sync::oneshot;

static SCHEDULE_RUNTIME: Mutex<Option<Handle>> = Mutex::new(None);
static THREAD_QUIT_SENDER: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);
static THREAD_JOIN_HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub async fn spawn_cert_generate_runtime() -> Option<RuntimeMetrics> {
    let (quit_sender, quit_receiver) = oneshot::channel();

    let (rt_handle_sender, rt_handle_receiver) = oneshot::channel();
    let Ok(thread_handle) = std::thread::Builder::new()
        .name("cert-generate".to_string())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };

            if rt_handle_sender.send(rt.handle().clone()).is_ok() {
                let _ = rt.block_on(quit_receiver);
            }
        })
    else {
        return None;
    };

    if let Ok(runtime_handle) = rt_handle_receiver.await {
        set_thread_quit_sender(quit_sender);
        set_thread_join_handle(thread_handle);
        set_cert_generate_rt_handle(runtime_handle.clone());
        Some(runtime_handle.metrics())
    } else {
        let _ = thread_handle.join();
        None
    }
}

pub fn close_cert_generate_runtime() {
    let mut lock = THREAD_QUIT_SENDER.lock().unwrap();
    if let Some(sender) = lock.take() {
        let _ = sender.send(());
    }
    drop(lock);

    let mut lock = THREAD_JOIN_HANDLE.lock().unwrap();
    if let Some(join_handle) = lock.take() {
        let _ = join_handle.join();
    }
}

fn set_thread_quit_sender(sender: oneshot::Sender<()>) {
    let mut lock = THREAD_QUIT_SENDER.lock().unwrap();
    *lock = Some(sender);
}

fn set_thread_join_handle(handle: JoinHandle<()>) {
    let mut lock = THREAD_JOIN_HANDLE.lock().unwrap();
    *lock = Some(handle);
}

fn set_cert_generate_rt_handle(handle: Handle) {
    let mut lock = SCHEDULE_RUNTIME.lock().unwrap();
    *lock = Some(handle);
}

pub fn get_cert_generate_rt_handle() -> Option<Handle> {
    let lock = SCHEDULE_RUNTIME.lock().unwrap();
    lock.as_ref().cloned()
}
