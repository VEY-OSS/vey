/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::nid::Nid;

use vey_crypto_mb::EcdsaCurve;

#[test]
fn ecdsa_p384_sign() {
    if !common::require_crypto_mb() {
        return;
    }

    let key = common::gen_ec(Nid::SECP384R1);
    common::test_ecdsa_sign(EcdsaCurve::P384, &key, &common::SHA256_DIGEST);
    common::test_ecdsa_sign(EcdsaCurve::P384, &key, &common::SHA384_DIGEST);
    common::test_ecdsa_sign(EcdsaCurve::P384, &key, &common::SHA512_DIGEST);
}
