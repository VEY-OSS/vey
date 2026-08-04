/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

//! High-level helpers around Intel `crypto_mb` multi-buffer primitives.

mod ecdsa;
mod ed25519;
mod ffi;
mod openssl_ffi;
mod rsa;

pub use ecdsa::{
    EcdsaP256Slot, EcdsaP384Slot, EcdsaP521Slot, EcdsaSlot, MAX_FIELD_LEN as ECDSA_MAX_FIELD_LEN,
    P256_FIELD_LEN as ECDSA_P256_FIELD_LEN, P384_FIELD_LEN as ECDSA_P384_FIELD_LEN,
    P521_FIELD_LEN as ECDSA_P521_FIELD_LEN, sign_mb8 as ecdsa_sign_mb8,
};
pub use ed25519::{
    Ed25519Slot, is_applicable as ed25519_is_applicable, sign_mb8 as ed25519_sign_mb8,
};
pub use rsa::{
    MAX_RSA_LEN as RSA_MAX_LEN, PreparedKind as RsaPreparedKind, RSA_2K_LEN, RSA_3K_LEN,
    RSA_4K_LEN, Rsa2kSlot, Rsa3kSlot, Rsa4kSlot, RsaSlot, add_pkcs1_sign_padding,
    add_pss_sign_padding, has_crt_params, private_crt_mb8, rsa_from_pkey,
};

pub const BATCH_SIZE: usize = ffi::BATCH_SIZE;
pub const MBX_STATUS_OK: MbStatus = ffi::MBX_STATUS_OK;
pub const MBX_STATUS_UNSUPPORTED_ISA_ERR: MbStatus = ffi::MBX_STATUS_UNSUPPORTED_ISA_ERR;
pub type MbStatus = ffi::MbStatus;

/// Returns whether the installed `crypto_mb` library can run on this CPU.
///
/// Calling multi-buffer kernels without a supported ISA (for example AVX-512
/// IFMA or AVX2-IFMA) may raise SIGILL.
pub fn is_applicable() -> bool {
    unsafe {
        let features = ffi::mbx_get_cpu_features();
        ffi::mbx_is_crypto_mb_applicable(features) != 0
    }
}

pub fn status_ok(status: MbStatus) -> bool {
    status == MBX_STATUS_OK
}
