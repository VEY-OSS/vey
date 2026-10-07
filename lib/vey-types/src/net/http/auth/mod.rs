/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;

use url::Url;

use crate::auth::{AuthParseError, Password, Username};
use crate::net::{AuthorizationScheme, AuthorizationValueParser, H1HeaderValue};

mod basic;
pub use basic::HttpBasicAuth;

pub enum HttpAuth {
    None,
    Basic(HttpBasicAuth),
}

impl HttpAuth {
    pub fn from_authorization(
        parsed: &AuthorizationValueParser<'_>,
    ) -> Result<Self, AuthParseError> {
        match parsed.scheme() {
            AuthorizationScheme::Basic => {
                let content = std::str::from_utf8(parsed.content())
                    .map_err(|_| AuthParseError::InvalidUtf8Encoding)?;
                let basic = HttpBasicAuth::from_str(content)?;
                Ok(HttpAuth::Basic(basic))
            }
            _ => Ok(HttpAuth::None),
        }
    }
}

impl TryFrom<&H1HeaderValue> for HttpAuth {
    type Error = AuthParseError;

    fn try_from(value: &H1HeaderValue) -> Result<Self, Self::Error> {
        let Some(parsed) = AuthorizationValueParser::parse(value.as_bytes()) else {
            return Err(AuthParseError::UnsupportedAuthType);
        };
        HttpAuth::from_authorization(&parsed)
    }
}

impl TryFrom<&Url> for HttpAuth {
    type Error = AuthParseError;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        let u = url.username();
        let auth = if u.is_empty() {
            HttpAuth::None
        } else {
            let username =
                Username::from_encoded(u).map_err(|_| AuthParseError::InvalidUsername)?;

            let password = if let Some(p) = url.password() {
                Password::from_encoded(p).map_err(|_| AuthParseError::InvalidPassword)?
            } else {
                return Err(AuthParseError::InvalidPassword);
            };

            HttpAuth::Basic(HttpBasicAuth::new(username, password))
        };

        Ok(auth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_header(value: &str) -> Result<HttpAuth, AuthParseError> {
        let parsed = AuthorizationValueParser::parse(value.as_bytes())
            .ok_or(AuthParseError::UnsupportedAuthType)?;
        HttpAuth::from_authorization(&parsed)
    }

    #[test]
    fn parse_ok() -> Result<(), ()> {
        let info = from_header("Basic cm9vdDp0b29y").unwrap();
        if let HttpAuth::Basic(HttpBasicAuth {
            username, password, ..
        }) = info
        {
            assert_eq!(username.as_original(), "root");
            assert_eq!(password.as_original(), "toor");
            Ok(())
        } else {
            Err(())
        }
    }

    #[test]
    fn parse_scheme_only() {
        let result = from_header("Basic ");
        assert!(result.is_err());
    }

    #[test]
    fn non_basic_scheme_is_none() {
        let info = from_header("Negotiate abc").unwrap();
        assert!(matches!(info, HttpAuth::None));
    }
}
