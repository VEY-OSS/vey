/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use openssl::md::Md;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::pkey_ctx::PkeyCtx;
use openssl::rsa::Padding;
use openssl::sign::RsaPssSaltlen;
use thiserror::Error;

use super::{KeylessDataResponse, KeylessErrorResponse, KeylessPongResponse};

pub(crate) mod cloudflare;

#[derive(Clone, Copy)]
pub(crate) enum KeylessAction {
    NotSet,
    Ping,
    RsaDecrypt(Padding),
    RsaSign(Nid),
    RsaPssSign(Nid),
    EcdsaSign(Nid),
    Ed25519Sign,
}

#[derive(Debug, Error)]
pub(crate) enum KeylessRequestError {
    #[error("closed early")]
    ClosedEarly,
    #[error("read failed: {0:?}")]
    ReadFailed(io::Error),
    #[error("invalid message length")]
    InvalidMessageLength,
    #[error("unexpected version {0}.{1}")]
    UnexpectedVersion(u8, u8),
    #[error("corrupted message")]
    CorruptedMessage,
    #[error("invalid length for item {0}")]
    InvalidItemLength(u8),
}

pub(crate) struct KeylessRequest {
    pub(crate) id: u32,
    pub(crate) opcode: u8,
    pub(crate) action: KeylessAction,
    pub(crate) ski: Vec<u8>,
    pub(crate) payload: Vec<u8>,
}

impl KeylessRequest {
    pub(super) fn new(id: u32) -> Self {
        KeylessRequest {
            id,
            opcode: 0,
            action: KeylessAction::NotSet,
            ski: Vec::new(),
            payload: Vec::new(),
        }
    }

    pub(crate) fn ping_pong(&self) -> Option<KeylessPongResponse> {
        if matches!(self.action, KeylessAction::Ping) {
            Some(KeylessPongResponse::new(self.id, &self.payload))
        } else {
            None
        }
    }

    pub(crate) fn find_key(&self) -> Result<PKey<Private>, KeylessErrorResponse> {
        if !self.ski.is_empty()
            && let Some(k) = crate::store::get_by_ski(&self.ski)
        {
            self.check_payload_for_key_size(k.size())?;
            return Ok(k);
        }
        Err(KeylessErrorResponse::new(self.id).key_not_found())
    }

    fn check_payload_for_key_size(&self, key_size: usize) -> Result<(), KeylessErrorResponse> {
        if matches!(self.action, KeylessAction::RsaDecrypt(_)) && self.payload.len() != key_size {
            return Err(KeylessErrorResponse::new(self.id).format_error());
        }
        Ok(())
    }

    pub(crate) fn process(
        &self,
        key: &PKey<Private>,
    ) -> Result<KeylessDataResponse, KeylessErrorResponse> {
        let key_size = key.size();
        let err_rsp = KeylessErrorResponse::new(self.id);
        let mut data_rsp = KeylessDataResponse::new(self.id, key_size);
        match self.action {
            KeylessAction::RsaDecrypt(p) => {
                let mut ctx = PkeyCtx::new(key).map_err(|_| err_rsp.crypto_fail())?;
                ctx.decrypt_init().map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_rsa_padding(p).map_err(|_| err_rsp.crypto_fail())?;

                let len = ctx
                    .decrypt(&self.payload, Some(data_rsp.payload_data_mut()))
                    .map_err(|_| err_rsp.crypto_fail())?;
                data_rsp.finalize_payload(len);
                Ok(data_rsp)
            }
            KeylessAction::RsaSign(h) => {
                let mut ctx = PkeyCtx::new(key).map_err(|_| err_rsp.crypto_fail())?;
                ctx.sign_init().map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_signature_md(Md::from_nid(h).unwrap())
                    .map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_rsa_padding(Padding::PKCS1)
                    .map_err(|_| err_rsp.crypto_fail())?;

                let len = ctx
                    .sign(&self.payload, Some(data_rsp.payload_data_mut()))
                    .map_err(|_| err_rsp.crypto_fail())?;
                data_rsp.finalize_payload(len);
                Ok(data_rsp)
            }
            KeylessAction::RsaPssSign(h) => {
                let mut ctx = PkeyCtx::new(key).map_err(|_| err_rsp.crypto_fail())?;
                ctx.sign_init().map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_signature_md(Md::from_nid(h).unwrap())
                    .map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_rsa_padding(Padding::PKCS1_PSS)
                    .map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_rsa_pss_saltlen(RsaPssSaltlen::DIGEST_LENGTH)
                    .map_err(|_| err_rsp.crypto_fail())?;

                let len = ctx
                    .sign(&self.payload, Some(data_rsp.payload_data_mut()))
                    .map_err(|_| err_rsp.crypto_fail())?;
                data_rsp.finalize_payload(len);
                Ok(data_rsp)
            }
            KeylessAction::EcdsaSign(h) => {
                let mut ctx = PkeyCtx::new(key).map_err(|_| err_rsp.crypto_fail())?;
                ctx.sign_init().map_err(|_| err_rsp.crypto_fail())?;
                ctx.set_signature_md(Md::from_nid(h).unwrap())
                    .map_err(|_| err_rsp.crypto_fail())?;

                let len = ctx
                    .sign(&self.payload, Some(data_rsp.payload_data_mut()))
                    .map_err(|_| err_rsp.crypto_fail())?;
                data_rsp.finalize_payload(len);
                Ok(data_rsp)
            }
            KeylessAction::Ed25519Sign => {
                let mut ctx = PkeyCtx::new(key).map_err(|_| err_rsp.crypto_fail())?;
                ctx.sign_init().map_err(|_| err_rsp.crypto_fail())?;

                let len = ctx
                    .sign(&self.payload, Some(data_rsp.payload_data_mut()))
                    .map_err(|_| err_rsp.crypto_fail())?;
                data_rsp.finalize_payload(len);
                Ok(data_rsp)
            }
            KeylessAction::NotSet | KeylessAction::Ping => Err(err_rsp.unexpected_op_code()),
        }
    }
}
