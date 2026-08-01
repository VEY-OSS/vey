/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::nid::Nid;

use vey_crypto_mb::EcdsaCurve;

#[test]
fn ecdsa_p256_sign() {
    if !common::require_crypto_mb() {
        return;
    }

    let key = common::gen_ec(Nid::X9_62_PRIME256V1);
    common::test_ecdsa_sign(EcdsaCurve::P256, &key, &common::SHA1_DIGEST);
    common::test_ecdsa_sign(EcdsaCurve::P256, &key, &common::SHA256_DIGEST);
    common::test_ecdsa_sign(EcdsaCurve::P256, &key, &common::SHA384_DIGEST);
}
