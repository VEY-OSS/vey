/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ptr;

use openssl::bn::{BigNum, BigNumContext};
use openssl::ecdsa::EcdsaSig;
use openssl::foreign_types::{ForeignType, ForeignTypeRef};
use openssl::pkey::{PKeyRef, Private};

use crate::MbStatus;
use crate::ffi::{self, BATCH_SIZE, MBX_STATUS_OK};
use crate::openssl_ffi;

/// Largest NIST curve field size we support (P-521).
pub const MAX_FIELD_LEN: usize = 66;

#[derive(Clone, Copy)]
pub enum Curve {
    P256,
    P384,
    P521,
}

impl Curve {
    pub fn field_len(self) -> usize {
        match self {
            Curve::P256 => 32,
            Curve::P384 => 48,
            Curve::P521 => 66,
        }
    }
}

pub struct EcdsaSlot {
    field_len: usize,
    msg: [u8; MAX_FIELD_LEN],
    sign_r: [u8; MAX_FIELD_LEN],
    sign_s: [u8; MAX_FIELD_LEN],
    eph: BigNum,
    /// Owned copy of the EC private scalar; must outlive `sign_mb8`.
    reg: BigNum,
}

impl EcdsaSlot {
    pub fn prepare(curve: Curve, key: &PKeyRef<Private>, digest: &[u8]) -> Option<Self> {
        let field_len = curve.field_len();
        let ec = key.ec_key().ok()?;
        let mut order = BigNum::new().ok()?;
        let mut ctx = BigNumContext::new().ok()?;
        ec.group().order(&mut order, &mut ctx).ok()?;
        // `ec_key()` returns an owned key; copy the scalar so it stays valid
        // after `ec` is dropped at the end of this function.
        let reg = ec.private_key().to_owned().ok()?;

        let mut msg = [0u8; MAX_FIELD_LEN];
        if digest.len() >= field_len {
            msg[..field_len].copy_from_slice(&digest[..field_len]);
        } else {
            msg[field_len - digest.len()..field_len].copy_from_slice(digest);
        }

        let eph = priv_rand_range(&order)?;
        Some(EcdsaSlot {
            field_len,
            msg,
            sign_r: [0u8; MAX_FIELD_LEN],
            sign_s: [0u8; MAX_FIELD_LEN],
            eph,
            reg,
        })
    }

    pub fn field_len(&self) -> usize {
        self.field_len
    }

    pub fn sign_r(&self) -> &[u8] {
        &self.sign_r[..self.field_len]
    }

    pub fn sign_s(&self) -> &[u8] {
        &self.sign_s[..self.field_len]
    }

    /// Encode the signature as DER.
    pub fn der_signature(&self) -> Option<Vec<u8>> {
        let r_bn = BigNum::from_slice(self.sign_r()).ok()?;
        let s_bn = BigNum::from_slice(self.sign_s()).ok()?;
        let sig = EcdsaSig::from_private_components(r_bn, s_bn).ok()?;
        sig.to_der().ok()
    }
}

/// Sign up to [`BATCH_SIZE`] slots. Returns per-lane status; only the first
/// `slots.len().min(BATCH_SIZE)` entries are meaningful.
pub fn sign_mb8(curve: Curve, slots: &mut [EcdsaSlot]) -> [MbStatus; BATCH_SIZE] {
    let n = slots.len().min(BATCH_SIZE);
    let mut statuses = [MBX_STATUS_OK; BATCH_SIZE];
    if n == 0 {
        return statuses;
    }

    let mut pa_sign_r = [ptr::null_mut(); BATCH_SIZE];
    let mut pa_sign_s = [ptr::null_mut(); BATCH_SIZE];
    let mut pa_msg = [ptr::null(); BATCH_SIZE];
    let mut pa_eph = [ptr::null(); BATCH_SIZE];
    let mut pa_reg = [ptr::null(); BATCH_SIZE];

    for (i, slot) in slots.iter_mut().take(n).enumerate() {
        pa_sign_r[i] = slot.sign_r.as_mut_ptr();
        pa_sign_s[i] = slot.sign_s.as_mut_ptr();
        pa_msg[i] = slot.msg.as_ptr();
        pa_eph[i] = slot.eph.as_ptr();
        pa_reg[i] = slot.reg.as_ptr();
    }

    let status = unsafe {
        match curve {
            Curve::P256 => ffi::mbx_nistp256_ecdsa_sign_ssl_mb8(
                pa_sign_r.as_ptr(),
                pa_sign_s.as_ptr(),
                pa_msg.as_ptr(),
                pa_eph.as_ptr(),
                pa_reg.as_ptr(),
                ptr::null_mut(),
            ),
            Curve::P384 => ffi::mbx_nistp384_ecdsa_sign_ssl_mb8(
                pa_sign_r.as_ptr(),
                pa_sign_s.as_ptr(),
                pa_msg.as_ptr(),
                pa_eph.as_ptr(),
                pa_reg.as_ptr(),
                ptr::null_mut(),
            ),
            Curve::P521 => ffi::mbx_nistp521_ecdsa_sign_ssl_mb8(
                pa_sign_r.as_ptr(),
                pa_sign_s.as_ptr(),
                pa_msg.as_ptr(),
                pa_eph.as_ptr(),
                pa_reg.as_ptr(),
                ptr::null_mut(),
            ),
        }
    };
    for (i, sts) in statuses.iter_mut().take(n).enumerate() {
        *sts = ffi::mbx_get_sts(status, i);
    }
    statuses
}

fn priv_rand_range(order: &openssl::bn::BigNumRef) -> Option<BigNum> {
    let eph = BigNum::new().ok()?;
    for _ in 0..64 {
        let rc = unsafe { openssl_ffi::BN_priv_rand_range(eph.as_ptr(), order.as_ptr()) };
        let is_zero = unsafe { openssl_ffi::BN_is_zero(eph.as_ptr()) == 1 };
        if rc == 1 && !is_zero {
            return Some(eph);
        }
    }
    None
}
