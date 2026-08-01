/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ptr;

use libc::c_int;
use openssl::foreign_types::ForeignTypeRef;
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{PKeyRef, Private};
use openssl::rsa::{Padding, RsaRef};
use openssl_sys::RSA_padding_check_PKCS1_type_2;

use crate::MbStatus;
use crate::ffi::{self, BATCH_SIZE, MBX_STATUS_OK};
use crate::openssl_ffi;

/// One RSA CRT slot for a multi-buffer private operation.
pub struct RsaCrtSlot<'a> {
    pub from: &'a [u8],
    pub to: &'a mut [u8],
    pub key: &'a RsaRef<Private>,
}

/// Run up to [`BATCH_SIZE`] RSA private CRT operations.
///
/// Returns per-lane status; only the first `slots.len().min(BATCH_SIZE)` entries
/// are meaningful. `MBX_STATUS_OK` means success.
pub fn private_crt_mb8(bits: i32, slots: &mut [RsaCrtSlot<'_>]) -> [MbStatus; BATCH_SIZE] {
    let n = slots.len().min(BATCH_SIZE);
    let mut statuses = [MBX_STATUS_OK; BATCH_SIZE];
    if n == 0 {
        return statuses;
    }

    let mut from_pa = [ptr::null(); BATCH_SIZE];
    let mut to_pa = [ptr::null_mut(); BATCH_SIZE];
    let mut p_pa = [ptr::null(); BATCH_SIZE];
    let mut q_pa = [ptr::null(); BATCH_SIZE];
    let mut dp_pa = [ptr::null(); BATCH_SIZE];
    let mut dq_pa = [ptr::null(); BATCH_SIZE];
    let mut iq_pa = [ptr::null(); BATCH_SIZE];

    for (i, slot) in slots.iter_mut().take(n).enumerate() {
        let Some((p, q, dp, dq, iq)) = crt_params(slot.key) else {
            // Missing CRT material: mark used lanes failed and skip the kernel.
            for sts in statuses.iter_mut().take(n) {
                *sts = 1;
            }
            return statuses;
        };
        from_pa[i] = slot.from.as_ptr();
        to_pa[i] = slot.to.as_mut_ptr();
        p_pa[i] = p;
        q_pa[i] = q;
        dp_pa[i] = dp;
        dq_pa[i] = dq;
        iq_pa[i] = iq;
    }

    let status = unsafe {
        ffi::mbx_rsa_private_crt_ssl_mb8(
            from_pa.as_ptr(),
            to_pa.as_ptr(),
            p_pa.as_ptr(),
            q_pa.as_ptr(),
            dp_pa.as_ptr(),
            dq_pa.as_ptr(),
            iq_pa.as_ptr(),
            bits,
        )
    };
    for (i, sts) in statuses.iter_mut().take(n).enumerate() {
        *sts = ffi::mbx_get_sts(status, i);
    }
    statuses
}

pub fn rsa_from_pkey(key: &PKeyRef<Private>) -> Option<openssl::rsa::Rsa<Private>> {
    key.rsa().ok()
}

pub fn has_crt_params(rsa: &RsaRef<Private>) -> bool {
    crt_params(rsa).is_some()
}

fn crt_params(
    rsa: &RsaRef<Private>,
) -> Option<(
    *const openssl_sys::BIGNUM,
    *const openssl_sys::BIGNUM,
    *const openssl_sys::BIGNUM,
    *const openssl_sys::BIGNUM,
    *const openssl_sys::BIGNUM,
)> {
    Some((
        rsa.p()?.as_ptr(),
        rsa.q()?.as_ptr(),
        rsa.dmp1()?.as_ptr(),
        rsa.dmq1()?.as_ptr(),
        rsa.iqmp()?.as_ptr(),
    ))
}

pub fn add_pkcs1_sign_padding(nid: Nid, digest: &[u8], em: &mut [u8]) -> bool {
    let tlen = em.len() as c_int;
    let rc = if nid == Nid::MD5_SHA1 {
        if digest.len() != 36 {
            return false;
        }
        unsafe {
            openssl_ffi::RSA_padding_add_PKCS1_type_1(
                em.as_mut_ptr(),
                tlen,
                digest.as_ptr(),
                digest.len() as c_int,
            )
        }
    } else {
        let Some(prefix) = digestinfo_prefix(nid) else {
            return false;
        };
        // Largest DigestInfo is SHA-512 prefix (19) + digest (64).
        let mut encoded = [0u8; 96];
        let encoded_len = prefix.len() + digest.len();
        if encoded_len > encoded.len() {
            return false;
        }
        encoded[..prefix.len()].copy_from_slice(prefix);
        encoded[prefix.len()..encoded_len].copy_from_slice(digest);
        unsafe {
            openssl_ffi::RSA_padding_add_PKCS1_type_1(
                em.as_mut_ptr(),
                tlen,
                encoded.as_ptr(),
                encoded_len as c_int,
            )
        }
    };
    rc == 1
}

pub fn add_pss_sign_padding(rsa: &RsaRef<Private>, nid: Nid, digest: &[u8], em: &mut [u8]) -> bool {
    let Some(md) = MessageDigest::from_nid(nid) else {
        return false;
    };
    let rc = unsafe {
        openssl_ffi::RSA_padding_add_PKCS1_PSS_mgf1(
            rsa.as_ptr(),
            em.as_mut_ptr(),
            digest.as_ptr(),
            md.as_ptr(),
            md.as_ptr(),
            -1,
        )
    };
    rc == 1
}

pub fn check_decrypt_padding(
    padding: Padding,
    from: &[u8],
    to: &mut [u8],
    rsa_len: usize,
) -> Option<usize> {
    let flen = from.len() as c_int;
    let tlen = to.len() as c_int;
    let num = rsa_len as c_int;
    let len = match padding {
        Padding::NONE => unsafe {
            openssl_ffi::RSA_padding_check_none(to.as_mut_ptr(), tlen, from.as_ptr(), flen, num)
        },
        Padding::PKCS1 => unsafe {
            RSA_padding_check_PKCS1_type_2(to.as_mut_ptr(), tlen, from.as_ptr(), flen, num)
        },
        _ => return None,
    };
    if len < 0 { None } else { Some(len as usize) }
}

/// DER prefix of the `DigestInfo` value `T` for EMSA-PKCS1-v1_5.
///
/// These are the fixed encodings from RFC 8017 (PKCS #1) for
/// `DigestInfo ::= SEQUENCE { digestAlgorithm AlgorithmIdentifier, digest OCTET STRING }`.
/// Callers append the raw digest bytes after the prefix, then apply PKCS#1 type-1 padding.
///
/// MD5-SHA1 (TLS legacy) is intentionally absent: that digest is padded without a DigestInfo
/// wrapper, matching OpenSSL `RSA_sign`.
fn digestinfo_prefix(nid: Nid) -> Option<&'static [u8]> {
    match nid {
        Nid::SHA1 => Some(&[
            0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04,
            0x14,
        ]),
        Nid::SHA224 => Some(&[
            0x30, 0x2d, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x04, 0x05, 0x00, 0x04, 0x1c,
        ]),
        Nid::SHA256 => Some(&[
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x01, 0x05, 0x00, 0x04, 0x20,
        ]),
        Nid::SHA384 => Some(&[
            0x30, 0x41, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x02, 0x05, 0x00, 0x04, 0x30,
        ]),
        Nid::SHA512 => Some(&[
            0x30, 0x51, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
            0x03, 0x05, 0x00, 0x04, 0x40,
        ]),
        _ => None,
    }
}
