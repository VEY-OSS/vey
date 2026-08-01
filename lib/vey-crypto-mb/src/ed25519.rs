/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ptr;

use openssl::pkey::{PKeyRef, Private};

use crate::MbStatus;
use crate::ffi::{self, BATCH_SIZE, MBX_STATUS_OK};

pub struct Ed25519Slot {
    private_key: [u8; 32],
    public_key: [u8; 32],
    /// Signature written by `sign_mb8` as `r || s`.
    sig: [u8; 64],
}

impl Ed25519Slot {
    pub fn prepare(key: &PKeyRef<Private>) -> Option<Self> {
        let priv_raw = key.raw_private_key().ok()?;
        let pub_raw = key.raw_public_key().ok()?;
        if priv_raw.len() != 32 || pub_raw.len() != 32 {
            return None;
        }
        let mut private_key = [0u8; 32];
        let mut public_key = [0u8; 32];
        private_key.copy_from_slice(&priv_raw);
        public_key.copy_from_slice(&pub_raw);
        Some(Ed25519Slot {
            private_key,
            public_key,
            sig: [0u8; 64],
        })
    }

    pub fn signature(&self) -> &[u8; 64] {
        &self.sig
    }
}

/// Sign up to [`BATCH_SIZE`] slots. `msgs.len()` must equal `slots.len()`.
///
/// Returns per-lane status; only the first `slots.len().min(BATCH_SIZE)` entries
/// are meaningful.
pub fn sign_mb8(slots: &mut [Ed25519Slot], msgs: &[&[u8]]) -> [MbStatus; BATCH_SIZE] {
    let n = slots.len().min(BATCH_SIZE).min(msgs.len());
    let mut statuses = [MBX_STATUS_OK; BATCH_SIZE];
    if n == 0 {
        return statuses;
    }

    let mut pa_sign_r = [ptr::null_mut(); BATCH_SIZE];
    let mut pa_sign_s = [ptr::null_mut(); BATCH_SIZE];
    let mut pa_msg = [ptr::null(); BATCH_SIZE];
    let mut pa_priv = [ptr::null(); BATCH_SIZE];
    let mut pa_pub = [ptr::null(); BATCH_SIZE];
    let mut msg_lens = [0u32; BATCH_SIZE];

    for (i, slot) in slots.iter_mut().take(n).enumerate() {
        let (r, s) = slot.sig.split_at_mut(32);
        pa_sign_r[i] = r.as_mut_ptr() as *mut ffi::Ed25519SignComponent;
        pa_sign_s[i] = s.as_mut_ptr() as *mut ffi::Ed25519SignComponent;
        pa_msg[i] = msgs[i].as_ptr();
        msg_lens[i] = msgs[i].len() as u32;
        pa_priv[i] = &slot.private_key;
        pa_pub[i] = &slot.public_key;
    }

    let status = unsafe {
        ffi::mbx_ed25519_sign_mb8(
            pa_sign_r.as_ptr(),
            pa_sign_s.as_ptr(),
            pa_msg.as_ptr(),
            msg_lens.as_ptr(),
            pa_priv.as_ptr(),
            pa_pub.as_ptr(),
        )
    };
    for (i, sts) in statuses.iter_mut().take(n).enumerate() {
        *sts = ffi::mbx_get_sts(status, i);
    }
    statuses
}
