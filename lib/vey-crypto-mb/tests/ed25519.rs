/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

mod common;

use openssl::pkey::{Id, PKey, Private};
use openssl::pkey_ctx::PkeyCtx;

use vey_crypto_mb::{Ed25519Slot, ed25519_sign_mb8, status_ok};

#[test]
fn ed25519_sign() {
    if !common::require_crypto_mb() {
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

    verify_ed25519(&key0, slots[0].signature(), msg0);
    verify_ed25519(&key1, slots[1].signature(), msg1);
}

/// `EVP_PKEY_verify` with the public key, matching vey-bench keyless `verify_ed`.
fn verify_ed25519(key: &PKey<Private>, sig: &[u8], msg: &[u8]) {
    let pub_raw = key.raw_public_key().expect("ed25519 public key");
    let public_key =
        PKey::public_key_from_raw_bytes(&pub_raw, Id::ED25519).expect("pkey from raw public");
    let mut ctx = PkeyCtx::new(&public_key).expect("pkey ctx");
    ctx.verify_init().expect("verify init");
    assert!(
        ctx.verify(msg, sig).expect("verify"),
        "Ed25519 EVP_PKEY_verify failed"
    );
}
