/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use libc::c_int;
use openssl_sys::BIGNUM;

pub type MbStatus = u32;

pub const MBX_STATUS_OK: MbStatus = 0;
pub const MBX_STATUS_UNSUPPORTED_ISA_ERR: MbStatus = 10;

pub const BATCH_SIZE: usize = 8;

pub(crate) type Ed25519SignComponent = [u8; 32];
pub(crate) type Ed25519PublicKey = [u8; 32];
pub(crate) type Ed25519PrivateKey = [u8; 32];

unsafe extern "C" {
    pub(crate) fn mbx_get_cpu_features() -> u64;
    pub(crate) fn mbx_is_crypto_mb_applicable(cpu_features: u64) -> c_int;

    pub(crate) fn mbx_rsa_private_crt_ssl_mb8(
        from_pa: *const *const u8,
        to_pa: *const *mut u8,
        p_pa: *const *const BIGNUM,
        q_pa: *const *const BIGNUM,
        dp_pa: *const *const BIGNUM,
        dq_pa: *const *const BIGNUM,
        iq_pa: *const *const BIGNUM,
        expected_rsa_bitsize: c_int,
    ) -> MbStatus;

    pub(crate) fn mbx_nistp256_ecdsa_sign_ssl_mb8(
        pa_sign_r: *const *mut u8,
        pa_sign_s: *const *mut u8,
        pa_msg: *const *const u8,
        pa_eph_skey: *const *const BIGNUM,
        pa_reg_skey: *const *const BIGNUM,
        p_buffer: *mut u8,
    ) -> MbStatus;

    pub(crate) fn mbx_nistp384_ecdsa_sign_ssl_mb8(
        pa_sign_r: *const *mut u8,
        pa_sign_s: *const *mut u8,
        pa_msg: *const *const u8,
        pa_eph_skey: *const *const BIGNUM,
        pa_reg_skey: *const *const BIGNUM,
        p_buffer: *mut u8,
    ) -> MbStatus;

    pub(crate) fn mbx_nistp521_ecdsa_sign_ssl_mb8(
        pa_sign_r: *const *mut u8,
        pa_sign_s: *const *mut u8,
        pa_msg: *const *const u8,
        pa_eph_skey: *const *const BIGNUM,
        pa_reg_skey: *const *const BIGNUM,
        p_buffer: *mut u8,
    ) -> MbStatus;

    pub(crate) fn mbx_ed25519_sign_mb8(
        pa_sign_r: *const *mut Ed25519SignComponent,
        pa_sign_s: *const *mut Ed25519SignComponent,
        pa_msg: *const *const u8,
        msg_len: *const u32,
        pa_private_key: *const *const Ed25519PrivateKey,
        pa_public_key: *const *const Ed25519PublicKey,
    ) -> MbStatus;

    pub(crate) fn mbx_ed25519_public_key_mb8(
        pa_public_key: *const *mut Ed25519PublicKey,
        pa_private_key: *const *const Ed25519PrivateKey,
    ) -> MbStatus;
}

#[inline]
pub(crate) fn mbx_get_sts(status: MbStatus, index: usize) -> MbStatus {
    (status >> (index * 4)) & 0xF
}
