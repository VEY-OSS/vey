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
