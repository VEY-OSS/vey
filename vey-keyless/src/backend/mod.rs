/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use openssl::pkey::{PKey, Private};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::config::backend::BackendDriverConfig;
use crate::serve::{WrappedKeylessRequest, WrappedKeylessResponse};

mod dispatch;
pub(crate) use dispatch::dispatch;

#[cfg(feature = "openssl-async-job")]
mod async_job;
#[cfg(feature = "openssl-async-job")]
pub(crate) use async_job::OpensslOperation;

#[cfg(all(feature = "crypto-mb", target_arch = "x86_64"))]
mod crypto_mb;

#[cfg(all(feature = "qat", target_os = "linux", target_arch = "x86_64"))]
mod qat;

mod simple;

pub(crate) struct DispatchedKeylessRequest {
    pub(crate) inner: WrappedKeylessRequest,
    pub(crate) key: PKey<Private>,
    pub(crate) rsp_sender: mpsc::Sender<WrappedKeylessResponse>,
}

trait Backend {
    async fn run_rsa_2048(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
    async fn run_rsa_3072(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
    async fn run_rsa_4096(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
    async fn run_ecdsa_p256(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
    async fn run_ecdsa_p384(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
    async fn run_ecdsa_p521(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
    async fn run_ed25519(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>);
}

pub fn create(id: usize, handle: &Handle) -> anyhow::Result<()> {
    let config = crate::config::backend::get_config();
    let channel_size = config.dispatch_channel_size;
    let counter_shift = config.dispatch_counter_shift;

    macro_rules! spawn_all {
        ($new_backend:expr) => {{
            macro_rules! setup {
                ($run:ident, $register:ident) => {{
                    let (sender, receiver) = mpsc::channel(channel_size);
                    handle.spawn(($new_backend).$run(receiver));
                    dispatch::$register(sender, counter_shift);
                }};
            }
            setup!(run_rsa_2048, register_rsa_2048);
            setup!(run_rsa_3072, register_rsa_3072);
            setup!(run_rsa_4096, register_rsa_4096);
            setup!(run_ecdsa_p256, register_ecdsa_p256);
            setup!(run_ecdsa_p384, register_ecdsa_p384);
            setup!(run_ecdsa_p521, register_ecdsa_p521);
            setup!(run_ed25519, register_ed25519);
        }};
    }

    match &config.driver {
        BackendDriverConfig::Simple => {
            log::debug!("starting simple backend on worker {id}");
            spawn_all!(simple::SimpleBackend::new());
        }
        #[cfg(feature = "openssl-async-job")]
        BackendDriverConfig::AsyncJob(driver) => {
            log::debug!("starting openssl async-job backend on worker {id}");
            let driver = *driver;
            spawn_all!(async_job::AsyncJobBackend::new(driver));
        }
        #[cfg(all(feature = "crypto-mb", target_arch = "x86_64"))]
        BackendDriverConfig::CryptoMb(driver) => {
            log::debug!("starting crypto-mb backend on worker {id}");
            let driver = *driver;
            spawn_all!(crypto_mb::CryptoMbBackend::new(driver));
        }
        #[cfg(all(feature = "qat", target_os = "linux", target_arch = "x86_64"))]
        BackendDriverConfig::Qat(driver) => {
            log::debug!("starting qat backend on worker {id}");
            // One QAT instance + poll task per worker; shared by all algo queues.
            let runtime = qat::QatBackend::create_runtime(driver, id, handle);
            let op_timeout = driver.op_timeout;
            spawn_all!(qat::QatBackend::new(runtime.clone(), op_timeout));
        }
    }

    Ok(())
}
