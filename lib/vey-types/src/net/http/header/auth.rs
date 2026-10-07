/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

/// Auth scheme of an `Authorization` or `WWW-Authenticate` field.
///
/// `Other` keeps the unrecognized scheme token from the input buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationScheme<'a> {
    Basic,
    Bearer,
    Digest,
    Negotiate,
    Ntlm,
    Other(&'a [u8]),
}

impl AuthorizationScheme<'_> {
    /// Connection-based schemes from RFC 4559.
    #[inline]
    pub fn is_session_based(&self) -> bool {
        matches!(self, Self::Negotiate | Self::Ntlm)
    }

    fn from_token(token: &[u8]) -> AuthorizationScheme<'_> {
        let Some((first, rest)) = token.split_first() else {
            return AuthorizationScheme::Other(token);
        };
        match first.to_ascii_lowercase() {
            b'b' if rest.eq_ignore_ascii_case(b"asic") => AuthorizationScheme::Basic,
            b'b' if rest.eq_ignore_ascii_case(b"earer") => AuthorizationScheme::Bearer,
            b'd' if rest.eq_ignore_ascii_case(b"igest") => AuthorizationScheme::Digest,
            b'n' if rest.eq_ignore_ascii_case(b"egotiate") => AuthorizationScheme::Negotiate,
            b'n' if rest.eq_ignore_ascii_case(b"tlm") => AuthorizationScheme::Ntlm,
            _ => AuthorizationScheme::Other(token),
        }
    }
}

/// Split `auth-scheme [ 1*SP content ]` without copying the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorizationValueParser<'a> {
    scheme: AuthorizationScheme<'a>,
    content: &'a [u8],
}

impl<'a> AuthorizationValueParser<'a> {
    pub fn parse(buf: &'a [u8]) -> Option<Self> {
        let buf = buf.trim_ascii_start();
        if buf.is_empty() {
            return None;
        }
        let scheme_end = memchr::memchr(b' ', buf).unwrap_or(buf.len());
        let token = &buf[..scheme_end];
        if token.is_empty() {
            return None;
        }
        Some(AuthorizationValueParser {
            scheme: AuthorizationScheme::from_token(token),
            content: buf[scheme_end..].trim_ascii_start(),
        })
    }

    #[inline]
    pub fn scheme(&self) -> AuthorizationScheme<'a> {
        self.scheme
    }

    /// Bytes after the scheme and its separating whitespace.
    #[inline]
    pub fn content(&self) -> &'a [u8] {
        self.content
    }

    #[inline]
    pub fn is_session_based(&self) -> bool {
        self.scheme.is_session_based()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_splits_scheme_and_content() {
        let parsed = AuthorizationValueParser::parse(b"Basic cm9vdDp0b29y").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Basic);
        assert_eq!(parsed.content(), b"cm9vdDp0b29y");
        assert!(!parsed.is_session_based());

        let parsed = AuthorizationValueParser::parse(b"Negotiate abc").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Negotiate);
        assert_eq!(parsed.content(), b"abc");
        assert!(parsed.is_session_based());

        let parsed = AuthorizationValueParser::parse(b"NTLM TlRMTVNTUA==").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Ntlm);
        assert_eq!(parsed.content(), b"TlRMTVNTUA==");

        let parsed = AuthorizationValueParser::parse(b"Digest realm=\"x\"").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Digest);
        assert_eq!(parsed.content(), b"realm=\"x\"");
        assert!(!parsed.is_session_based());

        let parsed = AuthorizationValueParser::parse(b"Bearer token").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Bearer);
        assert_eq!(parsed.content(), b"token");
        assert!(!parsed.is_session_based());

        let parsed = AuthorizationValueParser::parse(b"Custom token").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Other(b"Custom"));
        assert_eq!(parsed.content(), b"token");
    }

    #[test]
    fn parse_allows_scheme_only_and_leading_space() {
        let parsed = AuthorizationValueParser::parse(b"Negotiate").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Negotiate);
        assert!(parsed.content().is_empty());

        let parsed = AuthorizationValueParser::parse(b"  ntlm").unwrap();
        assert_eq!(parsed.scheme(), AuthorizationScheme::Ntlm);
        assert!(parsed.content().is_empty());
        assert!(parsed.is_session_based());

        assert!(AuthorizationValueParser::parse(b"   ").is_none());
        assert!(AuthorizationValueParser::parse(b"").is_none());
    }
}
