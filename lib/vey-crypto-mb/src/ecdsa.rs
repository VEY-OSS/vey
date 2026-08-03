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

pub const P256_FIELD_LEN: usize = 32;
pub const P384_FIELD_LEN: usize = 48;
pub const P521_FIELD_LEN: usize = 66;

/// Largest NIST curve field size we support (P-521).
pub const MAX_FIELD_LEN: usize = P521_FIELD_LEN;

pub struct EcdsaSlot<const N: usize> {
    msg: [u8; N],
    sign_r: [u8; N],
    sign_s: [u8; N],
    eph: BigNum,
    /// Owned copy of the EC private scalar; must outlive `sign_mb8`.
    reg: BigNum,
}

pub type EcdsaP256Slot = EcdsaSlot<P256_FIELD_LEN>;
pub type EcdsaP384Slot = EcdsaSlot<P384_FIELD_LEN>;
pub type EcdsaP521Slot = EcdsaSlot<P521_FIELD_LEN>;

impl<const N: usize> EcdsaSlot<N> {
    pub fn prepare(key: &PKeyRef<Private>, digest: &[u8]) -> Option<Self> {
        const {
            assert!(
                N == P256_FIELD_LEN || N == P384_FIELD_LEN || N == P521_FIELD_LEN,
                "unsupported ECDSA field length"
            );
        }

        let ec = key.ec_key().ok()?;
        let bits = ec.group().degree() as usize;
        let field_len = bits.div_ceil(8);
        if field_len != N {
            return None;
        }

        let mut order = BigNum::new().ok()?;
        let mut ctx = BigNumContext::new().ok()?;
        ec.group().order(&mut order, &mut ctx).ok()?;
        // `ec_key()` returns an owned key; copy the scalar so it stays valid
        // after `ec` is dropped at the end of this function.
        let reg = ec.private_key().to_owned().ok()?;

        let mut msg = [0u8; N];
        if digest.len() >= N {
            msg.copy_from_slice(&digest[..N]);
        } else {
            msg[N - digest.len()..].copy_from_slice(digest);
        }

        let eph = priv_rand_range(&order)?;
        Some(EcdsaSlot {
            msg,
            sign_r: [0u8; N],
            sign_s: [0u8; N],
            eph,
            reg,
        })
    }

    pub const fn field_len(&self) -> usize {
        N
    }

    pub fn sign_r(&self) -> &[u8; N] {
        &self.sign_r
    }

    pub fn sign_s(&self) -> &[u8; N] {
        &self.sign_s
    }

    /// Encode the signature as DER.
    pub fn der_signature(&self) -> Option<Vec<u8>> {
        let r_bn = BigNum::from_slice(self.sign_r.as_slice()).ok()?;
        let s_bn = BigNum::from_slice(self.sign_s.as_slice()).ok()?;
        let sig = EcdsaSig::from_private_components(r_bn, s_bn).ok()?;
        sig.to_der().ok()
    }
}

/// Sign up to [`BATCH_SIZE`] slots. Returns per-lane status; only the first
/// `slots.len().min(BATCH_SIZE)` entries are meaningful.
///
/// `N` must be [`P256_FIELD_LEN`], [`P384_FIELD_LEN`], or [`P521_FIELD_LEN`].
pub fn sign_mb8<const N: usize>(slots: &mut [EcdsaSlot<N>]) -> [MbStatus; BATCH_SIZE] {
    const {
        assert!(
            N == P256_FIELD_LEN || N == P384_FIELD_LEN || N == P521_FIELD_LEN,
            "unsupported ECDSA field length"
        );
    }

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
        match N {
            P256_FIELD_LEN => ffi::mbx_nistp256_ecdsa_sign_ssl_mb8(
                pa_sign_r.as_ptr(),
                pa_sign_s.as_ptr(),
                pa_msg.as_ptr(),
                pa_eph.as_ptr(),
                pa_reg.as_ptr(),
                ptr::null_mut(),
            ),
            P384_FIELD_LEN => ffi::mbx_nistp384_ecdsa_sign_ssl_mb8(
                pa_sign_r.as_ptr(),
                pa_sign_s.as_ptr(),
                pa_msg.as_ptr(),
                pa_eph.as_ptr(),
                pa_reg.as_ptr(),
                ptr::null_mut(),
            ),
            P521_FIELD_LEN => ffi::mbx_nistp521_ecdsa_sign_ssl_mb8(
                pa_sign_r.as_ptr(),
                pa_sign_s.as_ptr(),
                pa_msg.as_ptr(),
                pa_eph.as_ptr(),
                pa_reg.as_ptr(),
                ptr::null_mut(),
            ),
            _ => unreachable!(),
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
