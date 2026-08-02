/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::mem::MaybeUninit;

use vey_crypto_mb::{BATCH_SIZE, Ed25519Slot, ed25519_sign_mb8, status_ok};

use super::process_openssl;
use crate::backend::DispatchedKeylessRequest;
use crate::protocol::{KeylessAction, KeylessDataResponse, KeylessErrorResponse, KeylessResponse};

pub(super) async fn process_batch(batch: &mut Vec<DispatchedKeylessRequest>) {
    let n = batch.len();
    debug_assert!(n <= BATCH_SIZE);

    let mut prepared: [Option<Ed25519Slot>; BATCH_SIZE] = [const { None }; BATCH_SIZE];
    let mut fallback = [false; BATCH_SIZE];
    let mut mb_count = 0usize;

    for (i, req) in batch.iter().enumerate() {
        if !matches!(req.inner.inner.action, KeylessAction::Ed25519Sign) {
            fallback[i] = true;
            continue;
        }
        match Ed25519Slot::prepare(&req.key) {
            Some(p) => {
                prepared[i] = Some(p);
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
    let mut msgs: [&[u8]; BATCH_SIZE] = [&[]; BATCH_SIZE];
    let mut slots_buf: [MaybeUninit<Ed25519Slot>; BATCH_SIZE] =
        [const { MaybeUninit::uninit() }; BATCH_SIZE];
    let mut mb_n = 0usize;
    for (i, prep) in prepared.iter_mut().enumerate().take(n) {
        if let Some(slot) = prep.take() {
            msgs[mb_n] = batch[i].inner.inner.payload.as_ref();
            slots_buf[mb_n].write(slot);
            mb_indices[mb_n] = i;
            mb_n += 1;
        }
    }

    let statuses = {
        let slots = unsafe { slots_buf[..mb_n].assume_init_mut() };
        ed25519_sign_mb8(slots, &msgs[..mb_n])
    };

    let mut mb_slot_of = [BATCH_SIZE; BATCH_SIZE];
    for j in 0..mb_n {
        mb_slot_of[mb_indices[j]] = j;
    }

    for (i, req) in batch.drain(..).enumerate() {
        let rsp = if fallback[i] || mb_slot_of[i] >= mb_n {
            req.inner.process_by_openssl(&req.key)
        } else {
            let j = mb_slot_of[i];
            if !status_ok(statuses[j]) {
                req.inner.stats.add_crypto_fail();
                KeylessResponse::Error(KeylessErrorResponse::new(req.inner.inner.id).crypto_fail())
            } else {
                let slot = unsafe { slots_buf[j].assume_init_ref() };
                let data = KeylessDataResponse::with_payload(req.inner.inner.id, *slot.signature());
                req.inner.stats.add_passed();
                KeylessResponse::Data(data)
            }
        };
        let _ = req.rsp_sender.send(req.inner.build_response(rsp)).await;
    }

    unsafe {
        slots_buf[..mb_n].assume_init_drop();
    }
}
