/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::mem::MaybeUninit;

use openssl::pkey::{PKeyRef, Private};

use vey_crypto_mb::{
    BATCH_SIZE, RSA_2K_LEN, RSA_3K_LEN, RSA_4K_LEN, RsaSlot, private_crt_mb8, status_ok,
};

use super::process_openssl;
use crate::backend::DispatchedKeylessRequest;
use crate::protocol::{KeylessAction, KeylessDataResponse, KeylessErrorResponse, KeylessResponse};

pub(super) async fn process_batch(bits: i32, batch: &mut Vec<DispatchedKeylessRequest>) {
    match bits {
        2048 => process_sized::<RSA_2K_LEN>(batch).await,
        3072 => process_sized::<RSA_3K_LEN>(batch).await,
        4096 => process_sized::<RSA_4K_LEN>(batch).await,
        _ => {
            for req in batch.drain(..) {
                process_openssl(req).await;
            }
        }
    }
}

async fn process_sized<const N: usize>(batch: &mut Vec<DispatchedKeylessRequest>) {
    let n = batch.len();
    debug_assert!(n <= BATCH_SIZE);

    let mut prepared: [Option<RsaSlot<'_, N>>; BATCH_SIZE] = [const { None }; BATCH_SIZE];
    let mut msg_ids = [0u32; BATCH_SIZE];
    let mut fallback = [false; BATCH_SIZE];
    let mut mb_count = 0usize;

    for (i, req) in batch.iter().enumerate() {
        match prepare_slot(&req.key, &req.inner.inner.action, &req.inner.inner.payload) {
            Some(slot) => {
                prepared[i] = Some(slot);
                msg_ids[i] = req.inner.inner.id;
                mb_count += 1;
            }
            None => fallback[i] = true,
        }
    }

    if mb_count < 2 {
        for req in batch.drain(..) {
            process_openssl(req).await;
        }
        return;
    }

    let mut mb_indices = [0usize; BATCH_SIZE];
    let mut slots_buf: [MaybeUninit<RsaSlot<'_, N>>; BATCH_SIZE] =
        [const { MaybeUninit::uninit() }; BATCH_SIZE];
    let mut mb_n = 0usize;
    for (i, prep) in prepared.iter_mut().enumerate().take(n) {
        if let Some(slot) = prep.take() {
            slots_buf[mb_n].write(slot);
            mb_indices[mb_n] = i;
            mb_n += 1;
        }
    }

    let statuses = {
        let slots = unsafe {
            std::slice::from_raw_parts_mut(slots_buf.as_mut_ptr() as *mut RsaSlot<'_, N>, mb_n)
        };
        private_crt_mb8(slots)
    };

    // Finish while slots still borrow keys in `batch`.
    let mut results: [Option<Result<KeylessDataResponse, KeylessErrorResponse>>; BATCH_SIZE] =
        [const { None }; BATCH_SIZE];
    for j in 0..mb_n {
        let i = mb_indices[j];
        let slot = unsafe { slots_buf[j].assume_init_read() };
        if status_ok(statuses[j]) {
            results[i] = Some(finish_slot(slot, msg_ids[i]));
        }
    }

    let mut mb_slot_of = [BATCH_SIZE; BATCH_SIZE];
    for j in 0..mb_n {
        mb_slot_of[mb_indices[j]] = j;
    }

    for (i, req) in batch.drain(..).enumerate() {
        let rsp = if fallback[i] || mb_slot_of[i] >= mb_n {
            req.inner.process_by_openssl(&req.key)
        } else if !status_ok(statuses[mb_slot_of[i]]) {
            req.inner.stats.add_crypto_fail();
            KeylessResponse::Error(KeylessErrorResponse::new(req.inner.inner.id).crypto_fail())
        } else {
            match results[i].take().unwrap() {
                Ok(data) => {
                    req.inner.stats.add_passed();
                    KeylessResponse::Data(data)
                }
                Err(err) => {
                    req.inner.stats.add_by_error_code(err.error_code());
                    KeylessResponse::Error(err)
                }
            }
        };
        let _ = req.rsp_sender.send(req.inner.build_response(rsp)).await;
    }
}

fn prepare_slot<'a, const N: usize>(
    key: &'a PKeyRef<Private>,
    action: &KeylessAction,
    payload: &[u8],
) -> Option<RsaSlot<'a, N>> {
    match *action {
        KeylessAction::RsaDecrypt(padding) => RsaSlot::prepare_decrypt(key, payload, padding),
        KeylessAction::RsaSign(nid) => RsaSlot::prepare_pkcs1_sign(key, nid, payload),
        KeylessAction::RsaPssSign(nid) => RsaSlot::prepare_pss_sign(key, nid, payload),
        _ => None,
    }
}

fn finish_slot<const N: usize>(
    slot: RsaSlot<'_, N>,
    msg_id: u32,
) -> Result<KeylessDataResponse, KeylessErrorResponse> {
    let err = KeylessErrorResponse::new(msg_id);
    let (buf, len) = slot.into_output().ok_or_else(|| err.crypto_fail())?;
    Ok(KeylessDataResponse::with_payload(msg_id, &buf[..len]))
}
