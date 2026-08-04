/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::hash::MessageDigest;
use openssl::nid::Nid;

use vey_crypto_mb::{RSA_4K_LEN, RsaSlot, private_crt_mb8, status_ok};

const BITS: i32 = 4096;

#[test]
fn rsa_4k_sign_pkcs1() {
    if !common::require_crypto_mb() {
        return;
    }

    let key0 = common::gen_rsa(BITS as u32);
    let key1 = common::gen_rsa(BITS as u32);

    let mut slots = [
        RsaSlot::<RSA_4K_LEN>::prepare_pkcs1_sign(&key0, Nid::SHA256, &common::SHA256_DIGEST)
            .expect("prepare0"),
        RsaSlot::<RSA_4K_LEN>::prepare_pkcs1_sign(&key1, Nid::SHA256, &common::SHA256_DIGEST)
            .expect("prepare1"),
    ];
    let statuses = private_crt_mb8(&mut slots);
    assert!(
        statuses.iter().all(|s| status_ok(*s)),
        "statuses={statuses:?}"
    );

    let [s0, s1] = slots;
    let (sig0, len0) = s0.into_output().expect("out0");
    let (sig1, len1) = s1.into_output().expect("out1");

    common::verify_rsa_pkcs1(
        &key0,
        MessageDigest::sha256(),
        &sig0[..len0],
        &common::SHA256_DIGEST,
    );
    common::verify_rsa_pkcs1(
        &key1,
        MessageDigest::sha256(),
        &sig1[..len1],
        &common::SHA256_DIGEST,
    );
}
