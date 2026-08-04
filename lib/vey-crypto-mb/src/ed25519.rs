/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ptr;
use std::sync::OnceLock;

use openssl::foreign_types::ForeignTypeRef;
use openssl::pkey::{PKeyRef, Private};

use crate::MbStatus;
use crate::ffi::{self, BATCH_SIZE, MBX_STATUS_OK, MBX_STATUS_UNSUPPORTED_ISA_ERR};

#[derive(Clone)]
pub struct Ed25519Slot {
    private_key: [u8; 32],
    public_key: [u8; 32],
    /// Assembled signature `r || s` after a successful `sign_mb8`.
    sig: [u8; 64],
}

impl Ed25519Slot {
    pub fn prepare(key: &PKeyRef<Private>) -> Option<Self> {
        let mut private_key = [0u8; 32];
        let mut public_key = [0u8; 32];
        let mut priv_len = private_key.len();
        let mut pub_len = public_key.len();
        // Write straight into the slot buffers; avoid `raw_*_key()` Vec alloc/copy.
        unsafe {
            if openssl_sys::EVP_PKEY_get_raw_private_key(
                key.as_ptr(),
                private_key.as_mut_ptr(),
                &mut priv_len,
            ) != 1
                || priv_len != 32
            {
                return None;
            }
            if openssl_sys::EVP_PKEY_get_raw_public_key(
                key.as_ptr(),
                public_key.as_mut_ptr(),
                &mut pub_len,
            ) != 1
                || pub_len != 32
            {
                return None;
            }
        }
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

/// Wycheproof vector from Intel `fips_selftest_mbx_ed25519_sign_mb8` (empty message).
const KAT_PRIV: [u8; 32] = [
    0xad, 0xd4, 0xbb, 0x81, 0x03, 0x78, 0x5b, 0xaf, 0x9a, 0xc5, 0x34, 0x25, 0x8e, 0x8a, 0xaf, 0x65,
    0xf5, 0xf1, 0xad, 0xb5, 0xef, 0x5f, 0x3d, 0xf1, 0x9b, 0xb8, 0x0a, 0xb9, 0x89, 0xc4, 0xd6, 0x4b,
];
const KAT_PUB: [u8; 32] = [
    0x7d, 0x4d, 0x0e, 0x7f, 0x61, 0x53, 0xa6, 0x9b, 0x62, 0x42, 0xb5, 0x22, 0xab, 0xbe, 0xe6, 0x85,
    0xfd, 0xa4, 0x42, 0x0f, 0x88, 0x34, 0xb1, 0x08, 0xc3, 0xbd, 0xae, 0x36, 0x9e, 0xf5, 0x49, 0xfa,
];
const KAT_SIG: [u8; 64] = [
    0xd4, 0xfb, 0xdb, 0x52, 0xbf, 0xa7, 0x26, 0xb4, 0x4d, 0x17, 0x86, 0xa8, 0xc0, 0xd1, 0x71, 0xc3,
    0xe6, 0x2c, 0xa8, 0x3c, 0x9e, 0x5b, 0xbe, 0x63, 0xde, 0x0b, 0xb2, 0x48, 0x3f, 0x8f, 0xd6, 0xcc,
    0x14, 0x29, 0xab, 0x72, 0xca, 0xfc, 0x41, 0xab, 0x56, 0xaf, 0x02, 0xff, 0x8f, 0xcc, 0x43, 0xb9,
    0x9b, 0xfe, 0x4c, 0x7a, 0xe9, 0x40, 0xf6, 0x0f, 0x38, 0xeb, 0xaa, 0x9d, 0x31, 0x1c, 0x40, 0x07,
];

/// Whether Ed25519 multi-buffer kernels are safe to use on this host.
///
/// Requires a supported ISA **and** a passing Wycheproof/FIPS KAT. Some hosts
/// (notably certain cloud VMs that advertise AVX-512 IFMA) report success from
/// `mbx_ed25519_*` while producing incorrect public keys and signatures; the
/// KAT refuses to enable the path in that case.
pub fn is_applicable() -> bool {
    static OK: OnceLock<bool> = OnceLock::new();
    *OK.get_or_init(|| crate::is_applicable() && fips_kat_ok())
}

fn fips_kat_ok() -> bool {
    let mut derived = [[0u8; 32]; BATCH_SIZE];
    let privs = [KAT_PRIV; BATCH_SIZE];
    let pub_sts = public_key_mb8(&privs, &mut derived);
    if pub_sts.iter().any(|s| *s != MBX_STATUS_OK) || derived.iter().any(|p| *p != KAT_PUB) {
        return false;
    }

    let mut slots: [Ed25519Slot; BATCH_SIZE] = std::array::from_fn(|_| Ed25519Slot {
        private_key: KAT_PRIV,
        public_key: KAT_PUB,
        sig: [0u8; 64],
    });
    let msgs: [&[u8]; BATCH_SIZE] = [&[]; BATCH_SIZE];
    // Call the raw kernel directly: `sign_mb8` gates on `is_applicable()`, which
    // would re-enter this `OnceLock` initializer.
    let sts = sign_mb8_raw(&mut slots, &msgs);
    sts.iter().all(|s| *s == MBX_STATUS_OK) && slots.iter().all(|s| s.sig == KAT_SIG)
}

/// Sign up to [`BATCH_SIZE`] slots. `msgs.len()` must equal `slots.len()`.
///
/// Returns per-lane status; only the first `slots.len().min(BATCH_SIZE)` entries
/// are meaningful. Returns [`MBX_STATUS_UNSUPPORTED_ISA_ERR`] for every lane when
/// [`is_applicable`] is false.
pub fn sign_mb8(slots: &mut [Ed25519Slot], msgs: &[&[u8]]) -> [MbStatus; BATCH_SIZE] {
    if !is_applicable() {
        return [MBX_STATUS_UNSUPPORTED_ISA_ERR; BATCH_SIZE];
    }
    sign_mb8_raw(slots, msgs)
}

fn sign_mb8_raw(slots: &mut [Ed25519Slot], msgs: &[&[u8]]) -> [MbStatus; BATCH_SIZE] {
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

fn public_key_mb8(priv_keys: &[[u8; 32]], out_pubs: &mut [[u8; 32]]) -> [MbStatus; BATCH_SIZE] {
    let n = priv_keys.len().min(BATCH_SIZE).min(out_pubs.len());
    let mut statuses = [MBX_STATUS_OK; BATCH_SIZE];
    if n == 0 {
        return statuses;
    }

    let mut pa_priv = [ptr::null(); BATCH_SIZE];
    let mut pa_pub = [ptr::null_mut(); BATCH_SIZE];
    for i in 0..n {
        pa_priv[i] = &priv_keys[i];
        pa_pub[i] = &mut out_pubs[i];
    }

    let status = unsafe { ffi::mbx_ed25519_public_key_mb8(pa_pub.as_ptr(), pa_priv.as_ptr()) };
    for (i, sts) in statuses.iter_mut().take(n).enumerate() {
        *sts = ffi::mbx_get_sts(status, i);
    }
    statuses
}
