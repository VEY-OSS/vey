/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::cmp::Ordering;

use anyhow::anyhow;
use percent_encoding::{AsciiSet, CONTROLS};
use zeroize::Zeroizing;

const USERNAME_MAX_LENGTH: usize = u8::MAX as usize;
const PASSWORD_MAX_LENGTH: usize = u8::MAX as usize;

const USER_INFO_PCT_ENCODING_SET: &AsciiSet = &CONTROLS
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|');

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Username {
    inner: String,
    len: u8,
}

impl Username {
    pub fn empty() -> Self {
        Username {
            inner: String::new(),
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> u8 {
        self.len
    }

    pub fn from_original(s: &str) -> anyhow::Result<Self> {
        if s.is_empty() {
            return Err(anyhow!("empty username is not allowed"));
        }
        if s.len() > USERNAME_MAX_LENGTH {
            return Err(anyhow!("too long string for a username"));
        }
        if s.contains(':') {
            return Err(anyhow!("colon character is not allowed"));
        }
        Ok(Username {
            inner: s.to_string(),
            len: s.len() as u8,
        })
    }

    pub fn from_encoded(s: &str) -> anyhow::Result<Self> {
        let decoded = percent_encoding::percent_decode_str(s)
            .decode_utf8()
            .map_err(|e| anyhow!("decode failed: {e}"))?;
        Username::from_original(decoded.as_ref())
    }

    pub fn as_original(&self) -> &str {
        &self.inner
    }

    pub fn to_encoded(&self) -> String {
        Self::url_encode(self.as_original())
    }

    pub fn url_encode(original: &str) -> String {
        percent_encoding::utf8_percent_encode(original, USER_INFO_PCT_ENCODING_SET).to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Password {
    inner: Zeroizing<String>,
    len: u8,
}

impl Password {
    pub fn empty() -> Self {
        Password {
            inner: Zeroizing::new(String::new()),
            len: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> u8 {
        self.len
    }

    pub fn from_original(s: &str) -> anyhow::Result<Self> {
        if s.len() > PASSWORD_MAX_LENGTH {
            return Err(anyhow!("too long string for a password"));
        }
        Ok(Password {
            inner: Zeroizing::new(s.to_string()),
            len: s.len() as u8,
        })
    }

    pub fn from_encoded(s: &str) -> anyhow::Result<Self> {
        let decoded = percent_encoding::percent_decode_str(s)
            .decode_utf8()
            .map_err(|e| anyhow!("decode failed: {e}"))?;
        Password::from_original(decoded.as_ref())
    }

    pub fn as_original(&self) -> &str {
        &self.inner
    }

    pub fn to_encoded(&self) -> String {
        percent_encoding::utf8_percent_encode(self.as_original(), USER_INFO_PCT_ENCODING_SET)
            .to_string()
    }
}

impl PartialOrd for Password {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Password {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.as_str().cmp(other.inner.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_empty() {
        let username = Username::empty();
        assert!(username.is_empty());
        assert_eq!(username.len(), 0);
        assert_eq!(username.as_original(), "");
        assert_eq!(username.to_encoded(), "");
    }

    #[test]
    fn username_from_original_valid() {
        let username = Username::from_original("valid_username").unwrap();
        assert!(!username.is_empty());
        assert_eq!(username.len(), 14);
        assert_eq!(username.as_original(), "valid_username");
        assert_eq!(username.to_encoded(), "valid_username");
    }

    #[test]
    fn username_from_original_invalid_length() {
        let long_str = "a".repeat(USERNAME_MAX_LENGTH + 1);
        assert!(Username::from_original(&long_str).is_err());
    }

    #[test]
    fn username_from_original_with_colon() {
        assert!(Username::from_original("invalid:username").is_err());
    }

    #[test]
    fn username_from_encoded_valid() {
        let username = Username::from_encoded("valid%40user").unwrap();
        assert_eq!(username.as_original(), "valid@user");
        assert_eq!(username.to_encoded(), "valid%40user");
    }

    #[test]
    fn username_from_encoded_invalid() {
        assert!(Username::from_encoded("%FF").is_err());
    }

    #[test]
    fn username_encoding_roundtrip() {
        let original = "user@special/chars";
        let username = Username::from_original(original).unwrap();
        let encoded = username.to_encoded();
        let decoded = Username::from_encoded(&encoded).unwrap();
        assert_eq!(decoded, username);
    }

    #[test]
    fn password_empty() {
        let password = Password::empty();
        assert!(password.is_empty());
        assert_eq!(password.len(), 0);
        assert_eq!(password.as_original(), "");
        assert_eq!(password.to_encoded(), "");
    }

    #[test]
    fn password_from_original_valid() {
        let password = Password::from_original("pass:word").unwrap();
        assert!(!password.is_empty());
        assert_eq!(password.len(), 9);
        assert_eq!(password.as_original(), "pass:word");
    }

    #[test]
    fn password_from_original_invalid_length() {
        let long_str = "a".repeat(PASSWORD_MAX_LENGTH + 1);
        assert!(Password::from_original(&long_str).is_err());
    }

    #[test]
    fn password_from_encoded_valid() {
        let password = Password::from_encoded("pass%3Aword%40special").unwrap();
        assert_eq!(password.as_original(), "pass:word@special");
        assert_eq!(password.to_encoded(), "pass%3Aword%40special");
    }

    #[test]
    fn password_from_encoded_invalid() {
        assert!(Password::from_encoded("%FF").is_err());
    }

    #[test]
    fn password_encoding_roundtrip() {
        let original = "pass:word@special/chars";
        let password = Password::from_original(original).unwrap();
        let encoded = password.to_encoded();
        let decoded = Password::from_encoded(&encoded).unwrap();
        assert_eq!(decoded, password);
    }
}
