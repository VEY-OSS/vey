/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::hash::MessageDigest;
use openssl::nid::Nid;

use vey_crypto_mb::{RsaCrtSlot, add_pkcs1_sign_padding, private_crt_mb8, status_ok};

const BITS: i32 = 3072;
const KEY_LEN: usize = 384;

#[test]
fn rsa_3k_sign_pkcs1() {
    if !common::require_crypto_mb() {
        return;
    }

    let key0 = common::gen_rsa(BITS as u32);
    let key1 = common::gen_rsa(BITS as u32);
    let rsa0 = key0.rsa().unwrap();
    let rsa1 = key1.rsa().unwrap();

    let mut in0 = vec![0u8; KEY_LEN];
    let mut in1 = vec![0u8; KEY_LEN];
    let mut out0 = vec![0u8; KEY_LEN];
    let mut out1 = vec![0u8; KEY_LEN];

    assert!(add_pkcs1_sign_padding(
        Nid::SHA256,
        &common::SHA256_DIGEST,
        &mut in0
    ));
    assert!(add_pkcs1_sign_padding(
        Nid::SHA256,
        &common::SHA256_DIGEST,
        &mut in1
    ));

    let mut slots = vec![
        RsaCrtSlot {
            from: &in0,
            to: &mut out0,
            key: &rsa0,
        },
        RsaCrtSlot {
            from: &in1,
            to: &mut out1,
            key: &rsa1,
        },
    ];
    let statuses = private_crt_mb8(BITS, &mut slots);
    assert!(
        statuses.iter().all(|s| status_ok(*s)),
        "statuses={statuses:?}"
    );

    common::verify_rsa_pkcs1(
        &key0,
        MessageDigest::sha256(),
        &out0,
        &common::SHA256_DIGEST,
    );
    common::verify_rsa_pkcs1(
        &key1,
        MessageDigest::sha256(),
        &out1,
        &common::SHA256_DIGEST,
    );
}
