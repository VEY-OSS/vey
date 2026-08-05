/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::sync::mpsc;

use vey_crypto_mb::BATCH_SIZE;

use super::{Backend, DispatchedKeylessRequest};
use crate::config::backend::CryptoMbBackendConfig;

mod ecdsa;
mod ed25519;
mod rsa;

#[derive(Clone, Copy)]
enum CryptoMbKind {
    Rsa { bits: i32 },
    EcdsaP256,
    EcdsaP384,
    EcdsaP521,
    Ed25519,
}

pub(super) struct CryptoMbBackend {
    _config: CryptoMbBackendConfig,
    mb_applicable: bool,
    ed25519_applicable: bool,
}

impl CryptoMbBackend {
    pub(super) fn new(config: CryptoMbBackendConfig) -> Self {
        // Applicability checks log a warning once inside `vey-crypto-mb` when disabled.
        let mb_applicable = vey_crypto_mb::is_applicable();
        let ed25519_applicable = vey_crypto_mb::ed25519_is_applicable();
        CryptoMbBackend {
            _config: config,
            mb_applicable,
            ed25519_applicable,
        }
    }

    fn mb_enabled_for(&self, kind: CryptoMbKind) -> bool {
        match kind {
            CryptoMbKind::Ed25519 => self.ed25519_applicable,
            _ => self.mb_applicable,
        }
    }

    async fn run(self, kind: CryptoMbKind, mut receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        // `recv_many` requires a Vec; keep this as the only growable buffer.
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        loop {
            batch.clear();
            let n = receiver.recv_many(&mut batch, BATCH_SIZE).await;
            if n == 0 {
                break;
            }

            if n == 1 || !self.mb_enabled_for(kind) {
                for req in batch.drain(..) {
                    process_openssl(req).await;
                }
                continue;
            }

            match kind {
                CryptoMbKind::Rsa { bits } => rsa::process_batch(bits, &mut batch).await,
                CryptoMbKind::EcdsaP256 => ecdsa::process_p256_batch(&mut batch).await,
                CryptoMbKind::EcdsaP384 => ecdsa::process_p384_batch(&mut batch).await,
                CryptoMbKind::EcdsaP521 => ecdsa::process_p521_batch(&mut batch).await,
                CryptoMbKind::Ed25519 => ed25519::process_batch(&mut batch).await,
            }
        }
    }
}

pub(super) async fn process_openssl(req: DispatchedKeylessRequest) {
    let rsp = req.inner.process_by_openssl(&req.key);
    let _ = req.rsp_sender.send(req.inner.build_response(rsp)).await;
}

impl Backend for CryptoMbBackend {
    async fn run_rsa_2048(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::Rsa { bits: 2048 }, receiver).await
    }

    async fn run_rsa_3072(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::Rsa { bits: 3072 }, receiver).await
    }

    async fn run_rsa_4096(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::Rsa { bits: 4096 }, receiver).await
    }

    async fn run_ecdsa_p256(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::EcdsaP256, receiver).await
    }

    async fn run_ecdsa_p384(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::EcdsaP384, receiver).await
    }

    async fn run_ecdsa_p521(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::EcdsaP521, receiver).await
    }

    async fn run_ed25519(self, receiver: mpsc::Receiver<DispatchedKeylessRequest>) {
        self.run(CryptoMbKind::Ed25519, receiver).await
    }
}
