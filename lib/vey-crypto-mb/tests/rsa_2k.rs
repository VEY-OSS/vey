/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::rsa::Padding;

use vey_crypto_mb::{RSA_2K_LEN, RsaSlot, add_pkcs1_sign_padding, private_crt_mb8, status_ok};

const BITS: i32 = 2048;

#[test]
fn rsa_2k_sign_pkcs1() {
    if !common::require_crypto_mb() {
        return;
    }

    let key0 = common::gen_rsa(BITS as u32);
    let key1 = common::gen_rsa(BITS as u32);

    let mut slots = [
        RsaSlot::<RSA_2K_LEN>::prepare_pkcs1_sign(&key0, Nid::SHA256, &common::SHA256_DIGEST)
            .expect("prepare0"),
        RsaSlot::<RSA_2K_LEN>::prepare_pkcs1_sign(&key1, Nid::SHA256, &common::SHA256_DIGEST)
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
    assert_eq!(len0, RSA_2K_LEN);
    assert_eq!(len1, RSA_2K_LEN);

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

#[test]
fn rsa_2k_sign_pss() {
    if !common::require_crypto_mb() {
        return;
    }

    let key0 = common::gen_rsa(BITS as u32);
    let key1 = common::gen_rsa(BITS as u32);

    let mut slots = [
        RsaSlot::<RSA_2K_LEN>::prepare_pss_sign(&key0, Nid::SHA256, &common::SHA256_DIGEST)
            .expect("prepare0"),
        RsaSlot::<RSA_2K_LEN>::prepare_pss_sign(&key1, Nid::SHA256, &common::SHA256_DIGEST)
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

    common::verify_rsa_pss(
        &key0,
        MessageDigest::sha256(),
        &sig0[..len0],
        &common::SHA256_DIGEST,
    );
    common::verify_rsa_pss(
        &key1,
        MessageDigest::sha256(),
        &sig1[..len1],
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
    let mut ct0 = [0u8; RSA_2K_LEN];
    let mut ct1 = [0u8; RSA_2K_LEN];
    assert_eq!(
        rsa0.public_encrypt(plain, &mut ct0, Padding::PKCS1)
            .unwrap(),
        RSA_2K_LEN
    );
    assert_eq!(
        rsa1.public_encrypt(plain, &mut ct1, Padding::PKCS1)
            .unwrap(),
        RSA_2K_LEN
    );

    let mut slots = [
        RsaSlot::<RSA_2K_LEN>::prepare_decrypt(&key0, &ct0, Padding::PKCS1).expect("prepare0"),
        RsaSlot::<RSA_2K_LEN>::prepare_decrypt(&key1, &ct1, Padding::PKCS1).expect("prepare1"),
    ];
    let statuses = private_crt_mb8(&mut slots);
    assert!(
        statuses.iter().all(|s| status_ok(*s)),
        "statuses={statuses:?}"
    );

    let [s0, s1] = slots;
    let (buf0, n0) = s0.into_output().expect("unpad0");
    let (buf1, n1) = s1.into_output().expect("unpad1");
    assert_eq!(&buf0[..n0], plain);
    assert_eq!(&buf1[..n1], plain);
}

#[test]
fn rsa_2k_pkcs1_padding_roundtrip_openssl() {
    let key = common::gen_rsa(BITS as u32);
    let rsa = key.rsa().unwrap();
    let mut em = [0u8; RSA_2K_LEN];
    assert!(add_pkcs1_sign_padding(
        Nid::SHA256,
        &common::SHA256_DIGEST,
        &mut em
    ));
    let mut sig = [0u8; RSA_2K_LEN];
    assert_eq!(
        rsa.private_encrypt(&em, &mut sig, Padding::NONE).unwrap(),
        RSA_2K_LEN
    );
    common::verify_rsa_pkcs1(&key, MessageDigest::sha256(), &sig, &common::SHA256_DIGEST);
}
