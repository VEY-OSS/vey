/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::pkey::{PKey, Private};
use openssl::sign::{Signer, Verifier};

use vey_crypto_mb::{Ed25519Slot, ed25519_sign_mb8, status_ok};

#[test]
fn ed25519_sign() {
    if !common::require_ed25519() {
        return;
    }

    let key0 = PKey::generate_ed25519().expect("ed25519 key0");
    let key1 = PKey::generate_ed25519().expect("ed25519 key1");
    let msg0 = b"hello crypto_mb";
    let msg1 = b"batch ed25519";

    let mut slots = [
        Ed25519Slot::prepare(&key0).expect("prepare0"),
        Ed25519Slot::prepare(&key1).expect("prepare1"),
    ];
    let statuses = ed25519_sign_mb8(&mut slots, &[msg0.as_slice(), msg1.as_slice()]);
    assert!(
        status_ok(statuses[0]) && status_ok(statuses[1]),
        "statuses={statuses:?}"
    );

    // Ed25519 is deterministic: crypto_mb must match OpenSSL PureEdDSA.
    assert_eq!(slots[0].signature(), &openssl_ed25519_sign(&key0, msg0));
    assert_eq!(slots[1].signature(), &openssl_ed25519_sign(&key1, msg1));

    verify_ed25519(&key0, slots[0].signature(), msg0);
    verify_ed25519(&key1, slots[1].signature(), msg1);
}

fn openssl_ed25519_sign(key: &PKey<Private>, msg: &[u8]) -> [u8; 64] {
    let mut signer = Signer::new_without_digest(key).expect("signer");
    let sig = signer.sign_oneshot_to_vec(msg).expect("sign");
    let mut out = [0u8; 64];
    out.copy_from_slice(&sig);
    out
}

/// PureEdDSA verify via one-shot `EVP_DigestVerify` with NULL digest.
fn verify_ed25519(key: &PKey<Private>, sig: &[u8], msg: &[u8]) {
    let pub_der = key.public_key_to_der().expect("public der");
    let public_key = PKey::public_key_from_der(&pub_der).expect("public key");
    let mut verifier = Verifier::new_without_digest(&public_key).expect("verifier");
    assert!(
        verifier.verify_oneshot(sig, msg).expect("verify"),
        "Ed25519 verify failed"
    );
}
