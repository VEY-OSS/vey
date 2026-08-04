/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use libc::c_int;
use openssl_sys::{BIGNUM, EC_KEY, EVP_MD, EVP_PKEY, RSA};

unsafe extern "C" {
    /// Borrowed EC_KEY inside `pkey`; valid for the lifetime of `pkey`.
    pub(crate) fn EVP_PKEY_get0_EC_KEY(pkey: *const EVP_PKEY) -> *const EC_KEY;

    pub(crate) fn RSA_padding_add_PKCS1_type_1(
        to: *mut u8,
        tlen: c_int,
        from: *const u8,
        flen: c_int,
    ) -> c_int;

    pub(crate) fn RSA_padding_check_none(
        to: *mut u8,
        tlen: c_int,
        from: *const u8,
        flen: c_int,
        num: c_int,
    ) -> c_int;

    pub(crate) fn RSA_padding_add_PKCS1_PSS_mgf1(
        rsa: *mut RSA,
        em: *mut u8,
        m_hash: *const u8,
        hash: *const EVP_MD,
        mgf1_hash: *const EVP_MD,
        salt_len: c_int,
    ) -> c_int;

    pub(crate) fn BN_priv_rand_range(rnd: *mut BIGNUM, range: *const BIGNUM) -> c_int;
    pub(crate) fn BN_is_zero(a: *const BIGNUM) -> c_int;
}
