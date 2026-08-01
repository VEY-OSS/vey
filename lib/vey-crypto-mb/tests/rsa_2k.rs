/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::hash::MessageDigest;
use openssl::nid::Nid;

use vey_crypto_mb::{
    RsaCrtSlot, add_pkcs1_sign_padding, add_pss_sign_padding, check_decrypt_padding,
    private_crt_mb8, status_ok,
};

const BITS: i32 = 2048;
const KEY_LEN: usize = 256;

#[test]
fn rsa_2k_sign_pkcs1() {
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

#[test]
fn rsa_2k_sign_pss() {
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

    assert!(add_pss_sign_padding(
        &rsa0,
        Nid::SHA256,
        &common::SHA256_DIGEST,
        &mut in0
    ));
    assert!(add_pss_sign_padding(
        &rsa1,
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

    common::verify_rsa_pss(
        &key0,
        MessageDigest::sha256(),
        &out0,
        &common::SHA256_DIGEST,
    );
    common::verify_rsa_pss(
        &key1,
        MessageDigest::sha256(),
        &out1,
        &common::SHA256_DIGEST,
    );
}

#[test]
fn rsa_2k_private_decrypt() {
    if !common::require_crypto_mb() {
        return;
    }

    let key0 = common::gen_rsa(BITS as u32);
    let key1 = common::gen_rsa(BITS as u32);
    let rsa0 = key0.rsa().unwrap();
    let rsa1 = key1.rsa().unwrap();

    let plain = b"vey-crypto-mb rsa decrypt v1";
    let mut in0 = vec![0u8; KEY_LEN];
    let mut in1 = vec![0u8; KEY_LEN];
    assert_eq!(
        rsa0.public_encrypt(plain, &mut in0, openssl::rsa::Padding::PKCS1)
            .unwrap(),
        KEY_LEN
    );
    assert_eq!(
        rsa1.public_encrypt(plain, &mut in1, openssl::rsa::Padding::PKCS1)
            .unwrap(),
        KEY_LEN
    );

    let mut out0 = vec![0u8; KEY_LEN];
    let mut out1 = vec![0u8; KEY_LEN];
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

    let mut buf0 = vec![0u8; KEY_LEN];
    let mut buf1 = vec![0u8; KEY_LEN];
    let n0 = check_decrypt_padding(openssl::rsa::Padding::PKCS1, &out0, &mut buf0, KEY_LEN)
        .expect("unpad0");
    let n1 = check_decrypt_padding(openssl::rsa::Padding::PKCS1, &out1, &mut buf1, KEY_LEN)
        .expect("unpad1");
    assert_eq!(&buf0[..n0], plain);
    assert_eq!(&buf1[..n1], plain);
}

#[test]
fn rsa_2k_pkcs1_padding_roundtrip_openssl() {
    let key = common::gen_rsa(BITS as u32);
    let rsa = key.rsa().unwrap();
    let mut em = vec![0u8; KEY_LEN];
    assert!(add_pkcs1_sign_padding(
        Nid::SHA256,
        &common::SHA256_DIGEST,
        &mut em
    ));
    let mut sig = vec![0u8; KEY_LEN];
    assert_eq!(
        rsa.private_encrypt(&em, &mut sig, openssl::rsa::Padding::NONE)
            .unwrap(),
        KEY_LEN
    );
    common::verify_rsa_pkcs1(&key, MessageDigest::sha256(), &sig, &common::SHA256_DIGEST);
}
