/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::nid::Nid;

use vey_crypto_mb::EcdsaP256Slot;

#[test]
fn ecdsa_p256_sign() {
    if !common::require_crypto_mb() {
        return;
    }

    let key = common::gen_ec(Nid::X9_62_PRIME256V1);
    common::test_ecdsa_sign(
        &key,
        &common::SHA1_DIGEST,
        EcdsaP256Slot::prepare(&key, &common::SHA1_DIGEST).expect("prepare"),
    );
    common::test_ecdsa_sign(
        &key,
        &common::SHA256_DIGEST,
        EcdsaP256Slot::prepare(&key, &common::SHA256_DIGEST).expect("prepare"),
    );
    common::test_ecdsa_sign(
        &key,
        &common::SHA384_DIGEST,
        EcdsaP256Slot::prepare(&key, &common::SHA384_DIGEST).expect("prepare"),
    );
}

#[test]
fn ecdsa_p256_sign_above_order() {
    if !common::require_crypto_mb() {
        return;
    }

    let key = common::gen_ec(Nid::X9_62_PRIME256V1);
    let above = [0xffu8; 32];
    common::test_ecdsa_sign(
        &key,
        &above,
        EcdsaP256Slot::prepare(&key, &above).expect("prepare above order"),
    );
}

#[test]
fn ecdsa_p256_zero_digest_skips_crypto_mb() {
    // A hash-sized zero is a prepared digest, but crypto_mb rejects msg == 0.
    // `prepare` returns None so keyless falls back to OpenSSL.
    let key = common::gen_ec(Nid::X9_62_PRIME256V1);
    assert!(EcdsaP256Slot::prepare(&key, &[0u8; 32]).is_none());
}
