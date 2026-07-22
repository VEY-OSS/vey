/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::hash::Hasher;
use std::time::Duration;

use quinn_proto::{ConnectionId, ConnectionIdGenerator, InvalidCid};
use rustc_hash::FxHasher;

const CID_LENGTH: usize = 20;
const CID_COOKIE_LENGTH: usize = 8;
const CID_NONCE_LENGTH: usize = 4;

#[derive(Clone, Copy)]
pub struct QuinnReuseportIdGenerator {
    key: u64,
    cookie: u64,
    cid_lifetime: Option<Duration>,
}

impl QuinnReuseportIdGenerator {
    /// Create a new generator with a random key.
    pub fn new(cookie: u64) -> Self {
        let key = rand::random();
        QuinnReuseportIdGenerator {
            key,
            cookie,
            cid_lifetime: None,
        }
    }

    /// Create a new generator with a custom key and cookie.
    pub fn from_key(key: u64, cookie: u64) -> Self {
        QuinnReuseportIdGenerator {
            key,
            cookie,
            cid_lifetime: None,
        }
    }

    /// Set the lifetime of connection IDs created by this generator.
    pub fn set_lifetime(&mut self, lifetime: Duration) -> &mut Self {
        self.cid_lifetime = Some(lifetime);
        self
    }

    /// Set the lifetime of connection IDs using the builder pattern.
    pub fn with_lifetime(mut self, lifetime: Duration) -> Self {
        self.cid_lifetime = Some(lifetime);
        self
    }
}

impl ConnectionIdGenerator for QuinnReuseportIdGenerator {
    fn generate_cid(&mut self) -> ConnectionId {
        let mut buf = [0; CID_LENGTH];
        buf[..CID_COOKIE_LENGTH].copy_from_slice(&self.cookie.to_be_bytes());
        rand::fill(&mut buf[CID_COOKIE_LENGTH..CID_COOKIE_LENGTH + CID_NONCE_LENGTH]);

        let mut hasher = FxHasher::default();
        hasher.write_u64(self.key);
        hasher.write(&buf[..CID_COOKIE_LENGTH + CID_NONCE_LENGTH]);
        let hash = hasher.finish();
        buf[CID_COOKIE_LENGTH + CID_NONCE_LENGTH..].copy_from_slice(&hash.to_be_bytes());
        ConnectionId::new(&buf)
    }

    fn validate(&self, cid: &ConnectionId) -> Result<(), InvalidCid> {
        if cid.len() != CID_LENGTH {
            return Err(InvalidCid);
        }

        let cookie_bytes: [u8; CID_COOKIE_LENGTH] = cid[..CID_COOKIE_LENGTH]
            .try_into()
            .map_err(|_| InvalidCid)?;
        let cookie = u64::from_be_bytes(cookie_bytes);
        if cookie != self.cookie {
            return Err(InvalidCid);
        }

        let given_hash_bytes = &cid[CID_COOKIE_LENGTH + CID_NONCE_LENGTH..];

        let mut hasher = FxHasher::default();
        hasher.write_u64(self.key);
        hasher.write(&cid[..CID_COOKIE_LENGTH + CID_NONCE_LENGTH]);
        let expected_hash = hasher.finish();
        let expected_hash_bytes = expected_hash.to_be_bytes();

        if constant_time_eq::constant_time_eq(&expected_hash_bytes, given_hash_bytes) {
            Ok(())
        } else {
            Err(InvalidCid)
        }
    }

    fn cid_len(&self) -> usize {
        CID_LENGTH
    }

    fn cid_lifetime(&self) -> Option<Duration> {
        self.cid_lifetime
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_generation_and_validation() {
        let cookie = 0x123456789abcdef0;
        let mut generator = QuinnReuseportIdGenerator::new(cookie);

        assert_eq!(generator.cid_len(), CID_LENGTH);
        assert_eq!(generator.cid_lifetime(), None);

        let cid = generator.generate_cid();
        assert_eq!(cid.len(), CID_LENGTH);

        // Verify cookie is correctly stored in big-endian in the first 8 bytes.
        let cookie_bytes = &cid[..CID_COOKIE_LENGTH];
        assert_eq!(cookie_bytes, &cookie.to_be_bytes());

        // Verify validate succeeds for the generated CID
        assert!(generator.validate(&cid).is_ok());
    }

    #[test]
    fn test_validation_failures() {
        let cookie = 0x123456789abcdef0;
        let mut generator = QuinnReuseportIdGenerator::new(cookie);
        let cid = generator.generate_cid();

        // Length mismatch
        let mut short_cid = [0; 19];
        short_cid.copy_from_slice(&cid[..19]);
        assert!(generator.validate(&ConnectionId::new(&short_cid)).is_err());

        // Cookie mismatch
        let mut cid_mut = cid.to_vec();
        cid_mut[0] ^= 0xFF; // Modify first byte of cookie
        assert!(generator.validate(&ConnectionId::new(&cid_mut)).is_err());

        // Nonce mismatch (which changes the hash)
        let mut cid_mut2 = cid.to_vec();
        cid_mut2[CID_COOKIE_LENGTH] ^= 0xFF; // Modify first byte of nonce
        assert!(generator.validate(&ConnectionId::new(&cid_mut2)).is_err());

        // Hash mismatch
        let mut cid_mut3 = cid.to_vec();
        cid_mut3[CID_LENGTH - 1] ^= 0xFF; // Modify last byte of hash
        assert!(generator.validate(&ConnectionId::new(&cid_mut3)).is_err());
    }

    #[test]
    fn test_multiple_generators_isolation() {
        let cookie1 = 0x1111111111111111;
        let cookie2 = 0x2222222222222222;

        let mut gen1 = QuinnReuseportIdGenerator::new(cookie1);
        let mut gen2 = QuinnReuseportIdGenerator::new(cookie2);

        let cid1 = gen1.generate_cid();
        let cid2 = gen2.generate_cid();

        // Validating other's CID should fail
        assert!(gen1.validate(&cid2).is_err());
        assert!(gen2.validate(&cid1).is_err());

        // Generators with same cookie but different keys
        let mut gen3 = QuinnReuseportIdGenerator::new(cookie1);
        let cid3 = gen3.generate_cid();
        // Validation fails because key is different
        assert!(gen1.validate(&cid3).is_err());
    }

    #[test]
    fn test_lifetime_configuration() {
        let cookie = 12345;
        let generator = QuinnReuseportIdGenerator::new(cookie);
        assert_eq!(generator.cid_lifetime(), None);

        // Builder pattern (with_lifetime)
        let generator = generator.with_lifetime(Duration::from_secs(30));
        assert_eq!(generator.cid_lifetime(), Some(Duration::from_secs(30)));

        // Mutable setter pattern (set_lifetime)
        let mut generator = generator;
        generator.set_lifetime(Duration::from_secs(60));
        assert_eq!(generator.cid_lifetime(), Some(Duration::from_secs(60)));
    }

    #[test]
    fn test_from_key_consistency() {
        let key = 987654321;
        let cookie = 999999;

        let mut gen1 = QuinnReuseportIdGenerator::from_key(key, cookie);
        let gen2 = QuinnReuseportIdGenerator::from_key(key, cookie);

        let cid = gen1.generate_cid();

        // Since they share the key and cookie, validation should succeed
        assert!(gen2.validate(&cid).is_ok());
    }
}
