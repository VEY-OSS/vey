/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#![allow(dead_code)]

use openssl::bn::BigNum;
use openssl::ecdsa::EcdsaSig;
use openssl::hash::MessageDigest;
use openssl::md::Md;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private};
use openssl::pkey_ctx::PkeyCtx;
use openssl::rsa::Padding;
use openssl::sign::RsaPssSaltlen;

use vey_crypto_mb::{EcdsaSlot, ecdsa_sign_mb8, status_ok};

pub fn require_crypto_mb() -> bool {
    if vey_crypto_mb::is_applicable() {
        true
    } else {
        eprintln!("skip: crypto_mb is not applicable on this CPU");
        false
    }
}

/// Ed25519 additionally requires a passing FIPS/Wycheproof KAT. Some hosts
/// advertise AVX-512 IFMA but produce incorrect `mbx_ed25519_*` results.
pub fn require_ed25519() -> bool {
    if !require_crypto_mb() {
        return false;
    }
    if vey_crypto_mb::ed25519_is_applicable() {
        true
    } else {
        eprintln!(
            "skip: crypto_mb Ed25519 FIPS KAT failed (library/CPU produces incorrect results)"
        );
        false
    }
}

// Precomputed digests of:
//   SHA-1  ("vey-crypto-mb/tests sha1 digest v1")
//   SHA-256("vey-crypto-mb/tests sha256 digest v1")
//   SHA-384("vey-crypto-mb/tests sha384 digest v1")
//   SHA-512("vey-crypto-mb/tests sha512 digest v1")
pub const SHA1_DIGEST: [u8; 20] = [
    0xc8, 0xae, 0xba, 0x05, 0x88, 0x86, 0xc4, 0xf9, 0x9c, 0x4d, 0x1a, 0x5a, 0xaa, 0xce, 0x65, 0x1d,
    0xbc, 0x8b, 0x36, 0xe3,
];

pub const SHA256_DIGEST: [u8; 32] = [
    0x69, 0x42, 0xbd, 0xf8, 0xa9, 0xe1, 0x02, 0xb4, 0x78, 0xfc, 0x85, 0x97, 0x45, 0xcc, 0x86, 0x10,
    0xc1, 0xaa, 0xad, 0xea, 0x6c, 0xeb, 0xbd, 0xb8, 0x1e, 0xb2, 0x98, 0x36, 0xad, 0xa2, 0x70, 0x2f,
];

pub const SHA384_DIGEST: [u8; 48] = [
    0x2c, 0x28, 0x94, 0x19, 0xc8, 0x10, 0x02, 0xae, 0xc4, 0x7e, 0xcf, 0xd4, 0xf9, 0x21, 0xea, 0xf1,
    0x9d, 0x54, 0x35, 0xcd, 0xd4, 0xae, 0x73, 0x2f, 0x55, 0xf8, 0x2c, 0x88, 0x5d, 0x82, 0x69, 0x14,
    0xa1, 0x14, 0x12, 0xcf, 0x95, 0xc5, 0x70, 0x77, 0x45, 0x6e, 0x4d, 0x59, 0x57, 0xfb, 0x2e, 0x0c,
];

// SHA-512("vey-crypto-mb/tests sha512 digest v1")
pub const SHA512_DIGEST: [u8; 64] = [
    0x98, 0x64, 0x11, 0xdb, 0x06, 0x22, 0x9e, 0xff, 0x6a, 0x1d, 0xa0, 0x95, 0xa4, 0xda, 0xc0, 0x90,
    0x56, 0x16, 0x21, 0x89, 0x1e, 0xa8, 0xa4, 0xcb, 0x9d, 0x62, 0x2c, 0xa3, 0x98, 0xc6, 0xb2, 0x10,
    0xe7, 0x83, 0xef, 0x2d, 0x6e, 0xe4, 0xbd, 0xbe, 0x26, 0x3e, 0xb0, 0x82, 0xe7, 0x64, 0xec, 0x17,
    0x1b, 0xa7, 0x6d, 0xde, 0x65, 0x33, 0xcc, 0xd8, 0x68, 0x68, 0x5f, 0x88, 0xf1, 0xec, 0x73, 0xc2,
];

pub fn gen_rsa(bits: u32) -> PKey<Private> {
    let rsa = openssl::rsa::Rsa::generate(bits).expect("rsa keygen");
    PKey::from_rsa(rsa).expect("pkey from rsa")
}

pub fn gen_ec(nid: Nid) -> PKey<Private> {
    let group = openssl::ec::EcGroup::from_curve_name(nid).expect("ec group");
    let key = openssl::ec::EcKey::generate(&group).expect("ec keygen");
    PKey::from_ec_key(key).expect("pkey from ec")
}

/// `EVP_PKEY_verify` with PKCS#1 v1.5 padding.
pub fn verify_rsa_pkcs1(key: &PKey<Private>, md: MessageDigest, sig: &[u8], digest: &[u8]) {
    let mut ctx = PkeyCtx::new(key).expect("pkey ctx");
    ctx.verify_init().expect("verify init");
    ctx.set_rsa_padding(Padding::PKCS1).expect("padding");
    ctx.set_signature_md(Md::from_nid(md.type_()).unwrap())
        .expect("signature md");
    assert_eq!(
        ctx.verify(digest, sig).ok(),
        Some(true),
        "RSA PKCS1 verify failed"
    );
}

/// `EVP_PKEY_verify` with PSS. Salt length matches our padding helper (`salt_len = -1`
/// / digest length). Do not use `Verifier::new_without_digest` here.
pub fn verify_rsa_pss(key: &PKey<Private>, md: MessageDigest, sig: &[u8], digest: &[u8]) {
    let mut ctx = PkeyCtx::new(key).expect("pkey ctx");
    ctx.verify_init().expect("verify init");
    ctx.set_rsa_padding(Padding::PKCS1_PSS).expect("padding");
    ctx.set_signature_md(Md::from_nid(md.type_()).unwrap())
        .expect("signature md");
    ctx.set_rsa_mgf1_md(Md::from_nid(md.type_()).unwrap())
        .expect("mgf1 md");
    ctx.set_rsa_pss_saltlen(RsaPssSaltlen::DIGEST_LENGTH)
        .expect("saltlen");
    assert_eq!(
        ctx.verify(digest, sig).ok(),
        Some(true),
        "RSA PSS verify failed"
    );
}

/// Sign one digest with one key, then verify via `ECDSA_do_verify` and `EVP_PKEY_verify`.
pub fn test_ecdsa_sign<const N: usize>(key: &PKey<Private>, digest: &[u8], slot: EcdsaSlot<'_, N>) {
    let mut slots = [slot];
    let statuses = ecdsa_sign_mb8(&mut slots);
    assert!(status_ok(statuses[0]), "statuses={statuses:?}");

    let slot = &slots[0];
    let r = BigNum::from_slice(slot.sign_r()).expect("r");
    let s = BigNum::from_slice(slot.sign_s()).expect("s");
    let sig = EcdsaSig::from_private_components(r, s).expect("ECDSA_SIG");
    let ec = key.ec_key().expect("ec key");
    assert!(
        sig.verify(digest, &ec).expect("ECDSA_do_verify"),
        "ECDSA_do_verify failed"
    );

    let der = slot.der_signature().expect("der");
    let mut ctx = PkeyCtx::new(key).expect("pkey ctx");
    ctx.verify_init().expect("verify init");
    assert_eq!(
        ctx.verify(digest, &der).ok(),
        Some(true),
        "EVP_PKEY_verify failed"
    );
}
