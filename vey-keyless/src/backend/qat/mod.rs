/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::mpsc;

use vey_qat::{QatRuntime, ecdsa_sign_der, rsa_decrypt, rsa_sign_pkcs1, rsa_sign_pss};

use super::{Backend, DispatchedKeylessRequest};
use crate::config::backend::QatBackendConfig;
use crate::protocol::{KeylessAction, KeylessDataResponse, KeylessResponse};

pub(super) struct QatBackend {
    runtime: Option<Arc<QatRuntime>>,
    op_timeout: Duration,
}

impl QatBackend {
    /// Build a backend for one worker.
    ///
    /// Instance index comes from env `WORKER_<N>_QAT_INSTANCE` where `N` is
    /// `worker_id` (0-based), mapping into `cpaCyGetInstances`.
    pub(super) fn create_runtime(
        config: &QatBackendConfig,
        worker_id: usize,
        handle: &Handle,
    ) -> Option<Arc<QatRuntime>> {
        let instance_id = match instance_id_from_env(worker_id) {
            Ok(id) => id,
            Err(e) => {
                log::warn!(
                    "qat backend init failed for worker {worker_id}: {e}; using OpenSSL for this worker"
                );
                return None;
            }
        };
        let runtime = QatRuntime::try_new(&config.process_name, instance_id, handle);
        if runtime.is_none() {
            log::warn!(
                "qat backend init failed (process_name={:?}, instance_id={instance_id}, worker={worker_id}); using OpenSSL for this worker",
                config.process_name,
            );
        }
        runtime
    }

    pub(super) fn new(runtime: Option<Arc<QatRuntime>>, op_timeout: Duration) -> Self {
        QatBackend {
            runtime,
            op_timeout,
        }
    }

    async fn process_openssl(req: DispatchedKeylessRequest) {
        let rsp = req.inner.process_by_openssl(&req.key);
        let _ = req.rsp_sender.send(req.inner.build_response(rsp)).await;
    }

    async fn loop_run(self, mut receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        while let Some(req) = receiver.recv().await {
            let Some(runtime) = self.runtime.clone() else {
                Self::process_openssl(req).await;
                continue;
            };

            let DispatchedKeylessRequest {
                inner,
                key,
                rsp_sender,
            } = req;
            let msg_id = inner.inner.id;
            let action = inner.inner.action;
            let payload = inner.inner.payload.clone();
            let stats = inner.stats.clone();
            let timeout = self.op_timeout;

            tokio::spawn(async move {
                let qat = tokio::time::timeout(timeout, async {
                    match action {
                        KeylessAction::RsaDecrypt(padding) => {
                            rsa_decrypt(runtime, &key, &payload, padding)
                                .await
                                .map(|plain| KeylessDataResponse::with_payload(msg_id, plain))
                        }
                        KeylessAction::RsaSign(nid) => rsa_sign_pkcs1(runtime, &key, nid, &payload)
                            .await
                            .map(|sig| KeylessDataResponse::with_payload(msg_id, sig)),
                        KeylessAction::RsaPssSign(nid) => {
                            rsa_sign_pss(runtime, &key, nid, &payload)
                                .await
                                .map(|sig| KeylessDataResponse::with_payload(msg_id, sig))
                        }
                        KeylessAction::EcdsaSign(_) => ecdsa_sign_der(runtime, &key, &payload)
                            .await
                            .map(|der| KeylessDataResponse::with_payload(msg_id, der)),
                        _ => Err(vey_qat::Error::Prepare),
                    }
                })
                .await;

                let rsp = match qat {
                    Ok(Ok(data)) => {
                        stats.add_passed();
                        KeylessResponse::Data(data)
                    }
                    Ok(Err(_)) | Err(_) => {
                        // Timeout or QAT failure: synchronous OpenSSL fallback.
                        inner.process_by_openssl(&key)
                    }
                };
                let _ = rsp_sender.send(inner.build_response(rsp)).await;
            });
        }
    }
}

fn instance_id_from_env(worker_id: usize) -> anyhow::Result<u16> {
    let key = format!("WORKER_{worker_id}_QAT_INSTANCE");
    let value = std::env::var(&key)
        .map_err(|_| anyhow::anyhow!("env {key} is not set"))?;
    value
        .parse::<u16>()
        .map_err(|e| anyhow::anyhow!("env {key}={value:?} is not a valid u16: {e}"))
}

impl Backend for QatBackend {
    async fn run_rsa_2048(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.loop_run(receiver).await
    }

    async fn run_rsa_3072(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.loop_run(receiver).await
    }

    async fn run_rsa_4096(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.loop_run(receiver).await
    }

    async fn run_ecdsa_p256(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.loop_run(receiver).await
    }

    async fn run_ecdsa_p384(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.loop_run(receiver).await
    }

    async fn run_ecdsa_p521(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.loop_run(receiver).await
    }

    async fn run_ed25519(self, mut receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        // qatlib has no EdDSA sign; always OpenSSL.
        while let Some(req) = receiver.recv().await {
            Self::process_openssl(req).await;
        }
    }
}
