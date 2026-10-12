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
}
