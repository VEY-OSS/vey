/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::nid::Nid;

use vey_crypto_mb::EcdsaP384Slot;

#[test]
fn ecdsa_p384_sign() {
    if !common::require_crypto_mb() {
        return;
    }

    let key = common::gen_ec(Nid::SECP384R1);
    common::test_ecdsa_sign(
        &key,
        &common::SHA256_DIGEST,
        EcdsaP384Slot::prepare(&key, &common::SHA256_DIGEST).expect("prepare"),
    );
    common::test_ecdsa_sign(
        &key,
        &common::SHA384_DIGEST,
        EcdsaP384Slot::prepare(&key, &common::SHA384_DIGEST).expect("prepare"),
    );
    common::test_ecdsa_sign(
        &key,
        &common::SHA512_DIGEST,
        EcdsaP384Slot::prepare(&key, &common::SHA512_DIGEST).expect("prepare"),
    );
}
