/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::mem::MaybeUninit;

use openssl::pkey::{PKeyRef, Private};
use openssl::rsa::{Padding, Rsa};

use vey_crypto_mb::{
    BATCH_SIZE, RsaCrtSlot, add_pkcs1_sign_padding, add_pss_sign_padding, check_decrypt_padding,
    private_crt_mb8, rsa_from_pkey, status_ok,
};

use super::process_openssl;
use crate::backend::DispatchedKeylessRequest;
use crate::protocol::{KeylessAction, KeylessDataResponse, KeylessErrorResponse, KeylessResponse};

/// Largest RSA modulus we dispatch (4096-bit).
const MAX_RSA_LEN: usize = 512;

enum PreparedOut {
    /// CRT writes the signature directly into the response payload.
    Sign(KeylessDataResponse),
    /// CRT writes into this buffer; plaintext is unpadded into the response later.
    Decrypt {
        buf: [u8; MAX_RSA_LEN],
        padding: Padding,
    },
}

struct PreparedRsa {
    rsa: Rsa<Private>,
    input: [u8; MAX_RSA_LEN],
    key_len: usize,
    msg_id: u32,
    out: PreparedOut,
}

pub(super) async fn process_batch(bits: i32, batch: &mut Vec<DispatchedKeylessRequest>) {
    let n = batch.len();
    debug_assert!(n <= BATCH_SIZE);
    let key_len = (bits / 8) as usize;
    debug_assert!(key_len <= MAX_RSA_LEN);

    let mut prepared: [Option<PreparedRsa>; BATCH_SIZE] = [const { None }; BATCH_SIZE];
    let mut fallback = [false; BATCH_SIZE];
    let mut mb_count = 0usize;

    for (i, req) in batch.iter().enumerate() {
        match prepare_slot(
            key_len,
            req.inner.inner.id,
            &req.key,
            &req.inner.inner.action,
            &req.inner.inner.payload,
        ) {
            Ok(p) => {
                prepared[i] = Some(p);
                mb_count += 1;
            }
            Err(()) => fallback[i] = true,
        }
    }

    if mb_count < 2 {
        for req in batch.drain(..) {
            process_openssl(req).await;
        }
        return;
    }

    let mut mb_indices = [0usize; BATCH_SIZE];
    let mut mb_n = 0usize;
    for (i, prep) in prepared.iter().enumerate().take(n) {
        if prep.is_some() {
            mb_indices[mb_n] = i;
            mb_n += 1;
        }
    }

    let statuses = {
        let mut slots_buf: [MaybeUninit<RsaCrtSlot<'_>>; BATCH_SIZE] =
            [const { MaybeUninit::uninit() }; BATCH_SIZE];
        for j in 0..mb_n {
            // Disjoint indices; raw pointers avoid conflicting `prepared` borrows.
            let p = prepared[mb_indices[j]].as_mut().unwrap() as *mut PreparedRsa;
            unsafe {
                let key_len = (*p).key_len;
                let from = std::ptr::addr_of!((*p).input) as *const u8;
                let to = match &mut (*p).out {
                    PreparedOut::Sign(data) => data.payload_data_mut().as_mut_ptr(),
                    PreparedOut::Decrypt { buf, .. } => buf.as_mut_ptr(),
                };
                slots_buf[j].write(RsaCrtSlot {
                    from: std::slice::from_raw_parts(from, key_len),
                    to: std::slice::from_raw_parts_mut(to, key_len),
                    key: &(*p).rsa,
                });
            }
        }
        let slots = unsafe {
            std::slice::from_raw_parts_mut(slots_buf.as_mut_ptr() as *mut RsaCrtSlot<'_>, mb_n)
        };
        let statuses = private_crt_mb8(bits, slots);
        for slot in slots_buf.iter_mut().take(mb_n) {
            unsafe {
                slot.assume_init_drop();
            }
        }
        statuses
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
            let prep = prepared[i].take().unwrap();
            if !status_ok(statuses[j]) {
                req.inner.stats.add_crypto_fail();
                KeylessResponse::Error(KeylessErrorResponse::new(req.inner.inner.id).crypto_fail())
            } else {
                match finish_slot(prep) {
                    Ok(data) => {
                        req.inner.stats.add_passed();
                        KeylessResponse::Data(data)
                    }
                    Err(err) => {
                        req.inner.stats.add_by_error_code(err.error_code());
                        KeylessResponse::Error(err)
                    }
                }
            }
        };
        let _ = req.rsp_sender.send(req.inner.build_response(rsp)).await;
    }
}

fn prepare_slot(
    key_len: usize,
    msg_id: u32,
    key: &PKeyRef<Private>,
    action: &KeylessAction,
    payload: &[u8],
) -> Result<PreparedRsa, ()> {
    if key_len > MAX_RSA_LEN {
        return Err(());
    }
    let rsa = rsa_from_pkey(key).ok_or(())?;
    let mut input = [0u8; MAX_RSA_LEN];

    let out = match *action {
        KeylessAction::RsaDecrypt(padding) => {
            if payload.len() != key_len {
                return Err(());
            }
            input[..key_len].copy_from_slice(payload);
            PreparedOut::Decrypt {
                buf: [0u8; MAX_RSA_LEN],
                padding,
            }
        }
        KeylessAction::RsaSign(nid) => {
            if !add_pkcs1_sign_padding(nid, payload, &mut input[..key_len]) {
                return Err(());
            }
            PreparedOut::Sign(KeylessDataResponse::new(msg_id, key_len))
        }
        KeylessAction::RsaPssSign(nid) => {
            if !add_pss_sign_padding(&rsa, nid, payload, &mut input[..key_len]) {
                return Err(());
            }
            PreparedOut::Sign(KeylessDataResponse::new(msg_id, key_len))
        }
        _ => return Err(()),
    };

    Ok(PreparedRsa {
        rsa,
        input,
        key_len,
        msg_id,
        out,
    })
}

fn finish_slot(prep: PreparedRsa) -> Result<KeylessDataResponse, KeylessErrorResponse> {
    let PreparedRsa {
        key_len,
        msg_id,
        out,
        ..
    } = prep;
    match out {
        PreparedOut::Sign(mut data) => {
            data.finalize_payload(key_len);
            Ok(data)
        }
        PreparedOut::Decrypt { buf, padding } => {
            let err = KeylessErrorResponse::new(msg_id);
            let mut plain = [0u8; MAX_RSA_LEN];
            let len = check_decrypt_padding(padding, &buf[..key_len], &mut plain, key_len)
                .ok_or_else(|| err.crypto_fail())?;
            Ok(KeylessDataResponse::with_payload(msg_id, &plain[..len]))
        }
    }
}
