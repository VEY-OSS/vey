/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ptr;

use openssl::bn::{BigNum, BigNumContext, BigNumRef};
use openssl::ec::EcKeyRef;
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

pub struct EcdsaSlot<'a, const N: usize> {
    msg: [u8; N],
    sign_r: [u8; N],
    sign_s: [u8; N],
    eph: BigNum,
    /// Borrowed EC private scalar from the caller's `PKey`; must outlive `sign_mb8`.
    reg: &'a BigNumRef,
}

pub type EcdsaP256Slot<'a> = EcdsaSlot<'a, P256_FIELD_LEN>;
pub type EcdsaP384Slot<'a> = EcdsaSlot<'a, P384_FIELD_LEN>;
pub type EcdsaP521Slot<'a> = EcdsaSlot<'a, P521_FIELD_LEN>;

impl<'a, const N: usize> EcdsaSlot<'a, N> {
    pub fn prepare(key: &'a PKeyRef<Private>, digest: &[u8]) -> Option<Self> {
        const {
            assert!(
                N == P256_FIELD_LEN || N == P384_FIELD_LEN || N == P521_FIELD_LEN,
                "unsupported ECDSA field length"
            );
        }

        // Borrow the EC_KEY / private scalar from `key` (no get1 / BN_dup).
        let ec = unsafe {
            let ec_ptr = openssl_ffi::EVP_PKEY_get0_EC_KEY(key.as_ptr());
            if ec_ptr.is_null() {
                return None;
            }
            EcKeyRef::<Private>::from_ptr(ec_ptr as *mut _)
        };

        let bits = ec.group().degree() as usize;
        let field_len = bits.div_ceil(8);
        if field_len != N {
            return None;
        }

        let mut order = BigNum::new().ok()?;
        let mut ctx = BigNumContext::new().ok()?;
        ec.group().order(&mut order, &mut ctx).ok()?;
        let reg = ec.private_key();

        // Keyless payloads are already hash-sized. crypto_mb still requires
        // `0 < msg < order` and does not reduce, so a full-length digest can
        // be rejected (P-256 SHA-256 of 32×0xFF, or an all-zero digest).
        // Reduce modulo the order. Zero is not representable here; callers
        // such as keyless fall back to OpenSSL.
        let scalar = digest_reduced_mod_order(digest, &order)?;
        if bn_is_zero(&scalar) {
            return None;
        }
        let mut msg = [0u8; N];
        copy_be_fixed(&scalar, &mut msg)?;

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
pub fn sign_mb8<const N: usize>(slots: &mut [EcdsaSlot<'_, N>]) -> [MbStatus; BATCH_SIZE] {
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

/// OpenSSL's ECDSA integer: the leftmost `ceil(order_bits / 8)` bytes, with
/// leftover high bits shifted off when the order length is not a multiple of
/// 8, then reduced modulo the order.
fn digest_reduced_mod_order(digest: &[u8], order: &BigNumRef) -> Option<BigNum> {
    let order_bits = order.num_bits();
    if order_bits <= 0 {
        return None;
    }
    let order_bits_u = order_bits as usize;
    let mut len = digest.len();
    if len.saturating_mul(8) > order_bits_u {
        len = order_bits_u.div_ceil(8);
    }
    let mut m = BigNum::from_slice(&digest[..len]).ok()?;
    if len.saturating_mul(8) > order_bits_u {
        let mut shifted = BigNum::new().ok()?;
        shifted.rshift(&m, 8 - (order_bits & 7)).ok()?;
        m = shifted;
    }
    let mut reduced = BigNum::new().ok()?;
    let mut ctx = BigNumContext::new().ok()?;
    reduced.nnmod(&m, order, &mut ctx).ok()?;
    Some(reduced)
}

fn copy_be_fixed(bn: &BigNumRef, out: &mut [u8]) -> Option<()> {
    let bytes = bn.to_vec_padded(out.len() as i32).ok()?;
    out.copy_from_slice(&bytes);
    Some(())
}

fn bn_is_zero(bn: &BigNumRef) -> bool {
    unsafe { openssl_ffi::BN_is_zero(bn.as_ptr()) == 1 }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::*;

    fn assert_reduced(order: &[u8], digest: &[u8], expected: &[u8]) {
        let order = BigNum::from_slice(order).unwrap();
        let reduced = digest_reduced_mod_order(digest, &order).unwrap();
        assert_eq!(reduced.ucmp(&order), Ordering::Less);
        assert_eq!(
            reduced.to_vec_padded(expected.len() as i32).unwrap(),
            expected
        );
    }

    #[test]
    fn p256_digest_above_order_reduces() {
        // (2^256 - 1) mod n
        let order = [
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xbc, 0xe6, 0xfa, 0xad, 0xa7, 0x17, 0x9e, 0x84, 0xf3, 0xb9, 0xca, 0xc2,
            0xfc, 0x63, 0x25, 0x51,
        ];
        let expected = [
            0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x43, 0x19, 0x05, 0x52, 0x58, 0xe8, 0x61, 0x7b, 0x0c, 0x46, 0x35, 0x3d,
            0x03, 0x9c, 0xda, 0xae,
        ];
        assert_reduced(&order, &[0xff; 32], &expected);
        assert_reduced(&order, &[0u8; 32], &[0u8; 32]);
    }

    #[test]
    fn p521_digest_shifts_then_reduces() {
        // 66 * 0xFF is 528 bits. OpenSSL keeps the top 521 (>> 7), then mod n.
        let order = [
            0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xfa, 0x51, 0x86, 0x87, 0x83, 0xbf, 0x2f, 0x96, 0x6b,
            0x7f, 0xcc, 0x01, 0x48, 0xf7, 0x09, 0xa5, 0xd0, 0x3b, 0xb5, 0xc9, 0xb8, 0x89, 0x9c,
            0x47, 0xae, 0xbb, 0x6f, 0xb7, 0x1e, 0x91, 0x38, 0x64, 0x09,
        ];
        let mut expected = [0u8; 66];
        expected[33..].copy_from_slice(&[
            0x05, 0xae, 0x79, 0x78, 0x7c, 0x40, 0xd0, 0x69, 0x94, 0x80, 0x33, 0xfe, 0xb7, 0x08,
            0xf6, 0x5a, 0x2f, 0xc4, 0x4a, 0x36, 0x47, 0x76, 0x63, 0xb8, 0x51, 0x44, 0x90, 0x48,
            0xe1, 0x6e, 0xc7, 0x9b, 0xf6,
        ]);
        assert_reduced(&order, &[0xff; 66], &expected);
    }
}

fn priv_rand_range(order: &BigNumRef) -> Option<BigNum> {
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
