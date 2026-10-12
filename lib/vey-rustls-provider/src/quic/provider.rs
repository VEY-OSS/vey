/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use quinn::crypto::{AeadKey, CryptoError, HmacKey};
use rand::Rng;

cfg_select! {
    any(feature = "rustls-aws-lc", feature = "rustls-aws-lc-fips") => {
        use aws_lc_rs::{aead, hkdf, hmac};
    }
    feature = "rustls-ring" => {
        use ring::{aead, hkdf, hmac};
    }
}

/// HMAC-SHA256 key for QUIC stateless reset, from the active crypto library.
pub struct ResetKey(hmac::Key);

impl ResetKey {
    pub fn new() -> Self {
        let mut raw = [0u8; 64];
        rand::rng().fill_bytes(&mut raw);
        Self(hmac::Key::new(hmac::HMAC_SHA256, &raw))
    }
}

impl HmacKey for ResetKey {
    fn sign(&self, data: &[u8], signature_out: &mut [u8]) {
        signature_out.copy_from_slice(hmac::sign(&self.0, data).as_ref());
    }

    fn signature_len(&self) -> usize {
        32
    }

    fn verify(&self, data: &[u8], signature: &[u8]) -> Result<(), CryptoError> {
        hmac::verify(&self.0, data, signature).map_err(|_| CryptoError)
    }
}

/// HKDF-SHA256 key for QUIC handshake tokens, from the active crypto library.
pub struct HandshakeTokenKey(hkdf::Prk);

impl HandshakeTokenKey {
    pub fn new() -> Self {
        let mut master_key = [0u8; 64];
        rand::rng().fill_bytes(&mut master_key);
        Self(hkdf::Salt::new(hkdf::HKDF_SHA256, &[]).extract(&master_key))
    }
}

impl quinn::crypto::HandshakeTokenKey for HandshakeTokenKey {
    fn aead_from_hkdf(&self, random_bytes: &[u8]) -> Box<dyn AeadKey> {
        let mut key_buffer = [0u8; 32];
        let info = [random_bytes];
        let okm = self
            .0
            .expand(&info, hkdf::HKDF_SHA256)
            .expect("token key length");
        okm.fill(&mut key_buffer).expect("token key expand");

        let key = aead::UnboundKey::new(&aead::AES_256_GCM, &key_buffer).expect("AES-256-GCM");
        Box::new(TokenAead(aead::LessSafeKey::new(key)))
    }
}

struct TokenAead(aead::LessSafeKey);

impl AeadKey for TokenAead {
    fn seal(&self, data: &mut Vec<u8>, additional_data: &[u8]) -> Result<(), CryptoError> {
        let aad = aead::Aad::from(additional_data);
        let zero_nonce = aead::Nonce::assume_unique_for_key([0u8; 12]);
        self.0
            .seal_in_place_append_tag(zero_nonce, aad, data)
            .map_err(|_| CryptoError)
    }

    fn open<'a>(
        &self,
        data: &'a mut [u8],
        additional_data: &[u8],
    ) -> Result<&'a mut [u8], CryptoError> {
        let aad = aead::Aad::from(additional_data);
        let zero_nonce = aead::Nonce::assume_unique_for_key([0u8; 12]);
        self.0
            .open_in_place(zero_nonce, aad, data)
            .map_err(|_| CryptoError)
    }
}

const RETRY_INTEGRITY_KEY_DRAFT: [u8; 16] = [
    0xcc, 0xce, 0x18, 0x7e, 0xd0, 0x9a, 0x09, 0xd0, 0x57, 0x28, 0x15, 0x5a, 0x6c, 0xb9, 0x6b, 0xe1,
];
const RETRY_INTEGRITY_NONCE_DRAFT: [u8; 12] = [
    0xe5, 0x49, 0x30, 0xf9, 0x7f, 0x21, 0x36, 0xf0, 0x53, 0x0a, 0x8c, 0x1c,
];

const RETRY_INTEGRITY_KEY_V1: [u8; 16] = [
    0xbe, 0x0c, 0x69, 0x0b, 0x9f, 0x66, 0x57, 0x5a, 0x1d, 0x76, 0x6b, 0x54, 0xe3, 0x68, 0xc8, 0x4e,
];
const RETRY_INTEGRITY_NONCE_V1: [u8; 12] = [
    0x46, 0x15, 0x99, 0xd3, 0x5d, 0x63, 0x2b, 0xf2, 0x23, 0x98, 0x25, 0xbb,
];

pub(super) fn retry_key(version: rustls::quic::Version) -> ([u8; 12], [u8; 16]) {
    match version {
        rustls::quic::Version::V1 => (RETRY_INTEGRITY_NONCE_V1, RETRY_INTEGRITY_KEY_V1),
        rustls::quic::Version::V1Draft => (RETRY_INTEGRITY_NONCE_DRAFT, RETRY_INTEGRITY_KEY_DRAFT),
        _ => unreachable!(),
    }
}

pub(super) fn aes_128_gcm(key: &[u8; 16]) -> aead::LessSafeKey {
    aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, key).expect("AES-128-GCM"))
}

#[cfg(test)]
mod tests {
    use quinn::crypto::HandshakeTokenKey as _;
    use quinn::crypto::HmacKey;

    use super::*;

    #[test]
    fn reset_key_roundtrip() {
        let key = ResetKey::new();
        let mut signature = [0u8; 32];
        key.sign(b"stateless-reset", &mut signature);
        key.verify(b"stateless-reset", &signature).unwrap();
        assert!(key.verify(b"other", &signature).is_err());
    }

    #[test]
    fn token_aead_roundtrip() {
        let key = HandshakeTokenKey::new().aead_from_hkdf(b"random");
        let mut sealed = b"token".to_vec();
        key.seal(&mut sealed, b"aad").unwrap();
        let opened = key.open(&mut sealed, b"aad").unwrap();
        assert_eq!(opened, b"token");
    }

    #[test]
    fn retry_tag_roundtrip() {
        let version = rustls::quic::Version::V1;
        let (nonce, key) = retry_key(version);
        let key = aes_128_gcm(&key);
        let aad = b"retry-pseudo-packet";
        let tag = key
            .seal_in_place_separate_tag(
                aead::Nonce::assume_unique_for_key(nonce),
                aead::Aad::from(aad),
                &mut [],
            )
            .unwrap();
        let mut payload = tag.as_ref().to_vec();
        key.open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad),
            &mut payload,
        )
        .unwrap();
    }
}
