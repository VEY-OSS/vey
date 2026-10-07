/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use bytes::Bytes;
use http::{HeaderMap, HeaderValue};

use super::forwarded::{ForwardedValue, HttpForwardedHeaderType, X_FORWARDED_FOR};
use super::{AuthorizationValueParser, H1HeaderMap, H1HeaderValue};

pub trait HeaderMapExt {
    fn strip_forwarded(&mut self, ty: HttpForwardedHeaderType);
    fn append_forwarded(&mut self, value: &ForwardedValue, ty: HttpForwardedHeaderType);
    /// Remove `Forwarded` and `X-Forwarded-For` only.
    ///
    /// Drops the client-supplied client address and does not append this hop.
    fn steal_forwarded_for(&mut self);
    fn expect_100_continue(&self) -> bool;
    fn authorization_negotiate(&self) -> bool;
    fn maybe_grpc(&self) -> bool;
}

impl HeaderMapExt for HeaderMap {
    fn strip_forwarded(&mut self, ty: HttpForwardedHeaderType) {
        ty.for_each_header(|name| {
            self.remove(name);
        });
    }

    fn append_forwarded(&mut self, value: &ForwardedValue, ty: HttpForwardedHeaderType) {
        match ty {
            HttpForwardedHeaderType::Disable => {}
            HttpForwardedHeaderType::Classic => value.for_each_classic(|n, v| {
                self.append(n, unsafe { HeaderValue::from_maybe_shared_unchecked(v) });
            }),
            HttpForwardedHeaderType::Standard => {
                self.append(http::header::FORWARDED, unsafe {
                    HeaderValue::from_maybe_shared_unchecked(Bytes::from(value.standard_field()))
                });
            }
        }
    }

    fn steal_forwarded_for(&mut self) {
        self.remove(http::header::FORWARDED);
        self.remove(X_FORWARDED_FOR);
    }

    fn expect_100_continue(&self) -> bool {
        self.get_all(http::header::EXPECT)
            .iter()
            .any(|v| v.as_bytes() == b"100-continue")
    }

    fn authorization_negotiate(&self) -> bool {
        self.get_all(http::header::AUTHORIZATION).iter().any(|v| {
            AuthorizationValueParser::parse(v.as_bytes())
                .is_some_and(|auth| auth.is_session_based())
        })
    }

    fn maybe_grpc(&self) -> bool {
        self.get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(maybe_grpc_content_type)
    }
}

impl HeaderMapExt for H1HeaderMap {
    fn strip_forwarded(&mut self, ty: HttpForwardedHeaderType) {
        ty.for_each_header(|name| {
            self.remove(name);
        });
    }

    fn append_forwarded(&mut self, value: &ForwardedValue, ty: HttpForwardedHeaderType) {
        match ty {
            HttpForwardedHeaderType::Disable => {}
            HttpForwardedHeaderType::Classic => value.for_each_classic(|n, v| {
                self.append(n, unsafe { H1HeaderValue::from_buf_unchecked(v) });
            }),
            HttpForwardedHeaderType::Standard => {
                self.append(http::header::FORWARDED, unsafe {
                    H1HeaderValue::from_buf_unchecked(value.standard_field())
                });
            }
        }
    }

    fn steal_forwarded_for(&mut self) {
        self.remove(http::header::FORWARDED);
        self.remove(X_FORWARDED_FOR);
    }

    fn expect_100_continue(&self) -> bool {
        self.get_all(http::header::EXPECT)
            .iter()
            .any(|v| v.as_bytes() == b"100-continue")
    }

    fn authorization_negotiate(&self) -> bool {
        self.get_all(http::header::AUTHORIZATION).iter().any(|v| {
            AuthorizationValueParser::parse(v.as_bytes())
                .is_some_and(|auth| auth.is_session_based())
        })
    }

    fn maybe_grpc(&self) -> bool {
        self.get(http::header::CONTENT_TYPE)
            .is_some_and(|v| maybe_grpc_content_type(v.to_str()))
    }
}

fn maybe_grpc_content_type(value: &str) -> bool {
    const PREFIX: &[u8] = b"application/grpc";
    let v = value.trim_ascii_start().as_bytes();
    if v.len() < PREFIX.len() || !v[..PREFIX.len()].eq_ignore_ascii_case(PREFIX) {
        return false;
    }
    matches!(v.get(PREFIX.len()), None | Some(b'+' | b';' | b' ' | b'\t'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steal_forwarded_for_removes_identity_headers() {
        let mut map = HeaderMap::new();
        map.append(
            "forwarded",
            HeaderValue::from_static("for=192.0.2.8; proto=https"),
        );
        map.append("x-forwarded-for", HeaderValue::from_static("203.0.113.9"));
        map.append("x-forwarded-proto", HeaderValue::from_static("https"));
        map.append("host", HeaderValue::from_static("keep.example"));

        map.steal_forwarded_for();

        assert!(map.get(http::header::FORWARDED).is_none());
        assert!(map.get("x-forwarded-for").is_none());
        assert_eq!(map.get("x-forwarded-proto").unwrap(), "https");
        assert_eq!(map.get("host").unwrap(), "keep.example");
    }

    #[test]
    fn h1_steal_forwarded_for_removes_identity_headers() {
        let mut map = H1HeaderMap::default();
        map.append(
            http::header::FORWARDED,
            H1HeaderValue::from_static("for=192.0.2.8; proto=https"),
        );
        map.append(X_FORWARDED_FOR, H1HeaderValue::from_static("203.0.113.9"));
        map.append(
            http::HeaderName::from_static("x-forwarded-proto"),
            H1HeaderValue::from_static("https"),
        );
        map.append(
            http::header::HOST,
            H1HeaderValue::from_static("keep.example"),
        );

        map.steal_forwarded_for();

        assert!(map.get(http::header::FORWARDED).is_none());
        assert!(map.get(X_FORWARDED_FOR).is_none());
        assert_eq!(map.get("x-forwarded-proto").unwrap().to_str(), "https");
        assert_eq!(
            map.get(http::header::HOST).unwrap().to_str(),
            "keep.example"
        );
    }

    #[test]
    fn expect_100_continue_detects_header() {
        let mut map = HeaderMap::new();
        map.append(
            http::header::EXPECT,
            HeaderValue::from_static("100-continue"),
        );
        assert!(map.expect_100_continue());

        let mut map = HeaderMap::new();
        map.append(http::header::EXPECT, HeaderValue::from_static("other"));
        assert!(!map.expect_100_continue());

        let mut map = H1HeaderMap::default();
        map.append(
            http::header::EXPECT,
            H1HeaderValue::from_static("100-continue"),
        );
        assert!(map.expect_100_continue());
    }

    #[test]
    fn authorization_negotiate_detects_session_auth() {
        let mut map = HeaderMap::new();
        map.append(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Negotiate abc"),
        );
        assert!(map.authorization_negotiate());

        let mut map = HeaderMap::new();
        map.append(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("NTLM TlRMTVNTUA=="),
        );
        assert!(map.authorization_negotiate());

        let mut map = HeaderMap::new();
        map.append(
            http::header::AUTHORIZATION,
            HeaderValue::from_static("Basic abc"),
        );
        assert!(!map.authorization_negotiate());

        let mut map = H1HeaderMap::default();
        map.append(
            http::header::AUTHORIZATION,
            H1HeaderValue::from_static("Negotiate abc"),
        );
        assert!(map.authorization_negotiate());
    }

    #[test]
    fn maybe_grpc_detects_content_type() {
        let mut map = HeaderMap::new();
        map.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/grpc"),
        );
        assert!(map.maybe_grpc());

        map.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/grpc+proto"),
        );
        assert!(map.maybe_grpc());

        map.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("APPLICATION/GRPC; charset=utf-8"),
        );
        assert!(map.maybe_grpc());

        map.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/grpc-web+proto"),
        );
        assert!(!map.maybe_grpc());

        map.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        assert!(!map.maybe_grpc());

        let mut map = H1HeaderMap::default();
        map.insert(
            http::header::CONTENT_TYPE,
            H1HeaderValue::from_static("application/grpc+proto"),
        );
        assert!(map.maybe_grpc());
    }
}
