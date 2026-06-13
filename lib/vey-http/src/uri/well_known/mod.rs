/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use http::Uri;
use smol_str::SmolStr;

use vey_types::net::{HttpProxySubProtocol, UpstreamAddr};

use super::{HttpMasque, UriParseError};

mod easy_proxy;
mod masque;

#[derive(Debug)]
pub enum WellKnownUri {
    EasyProxy(HttpProxySubProtocol, UpstreamAddr, Uri),
    Masque(HttpMasque),
    Unsupported(SmolStr),
}

struct WellKnownUriParser<'a> {
    uri: &'a Uri,
    path_offset: usize,
}

impl<'a> WellKnownUriParser<'a> {
    pub fn new(uri: &'a Uri) -> Self {
        WellKnownUriParser {
            uri,
            path_offset: 0,
        }
    }

    pub fn parse(mut self) -> Result<Option<WellKnownUri>, UriParseError> {
        let Some(magic) = self.next_path_segment() else {
            return Ok(None);
        };
        if magic != ".well-known" {
            return Ok(None);
        }

        let Some(name) = self.next_path_segment() else {
            return Ok(None);
        };
        let v = match name {
            "easy-proxy" => self.parse_easy_proxy()?,
            "masque" => self.parse_masque()?,
            _ => WellKnownUri::Unsupported(SmolStr::from(name)),
        };
        Ok(Some(v))
    }

    fn next_path_segment(&mut self) -> Option<&'a str> {
        loop {
            let left = &self.uri.path()[self.path_offset..];
            if left.is_empty() {
                return None;
            }

            match memchr::memchr(b'/', left.as_bytes()) {
                Some(0) => self.path_offset += 1,
                Some(p) => {
                    self.path_offset += p + 1;
                    return Some(&left[..p]);
                }
                None => return Some(left),
            }
        }
    }
}

impl WellKnownUri {
    pub fn parse(uri: &Uri) -> Result<Option<WellKnownUri>, UriParseError> {
        WellKnownUriParser::new(uri).parse()
    }

    pub fn suffix(&self) -> &str {
        match self {
            WellKnownUri::EasyProxy(_, _, _) => "easy-proxy",
            WellKnownUri::Masque(_) => "masque",
            WellKnownUri::Unsupported(s) => s.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_path_segment_skips_empty_segments() {
        let uri = Uri::from_static("///a//b/c///");
        let mut parser = WellKnownUriParser::new(&uri);

        assert_eq!(parser.next_path_segment(), Some("a"));
        assert_eq!(parser.next_path_segment(), Some("b"));
        assert_eq!(parser.next_path_segment(), Some("c"));
        assert_eq!(parser.next_path_segment(), None);
    }

    #[test]
    fn parse_returns_none_for_non_well_known_paths() {
        for uri in [
            Uri::default(),
            Uri::from_static("/"),
            Uri::from_static("///"),
            Uri::from_static("/other-path"),
            Uri::from_static("/.well-known/"),
        ] {
            assert!(WellKnownUri::parse(&uri).unwrap().is_none());
        }
    }

    #[test]
    fn parse_unsupported_well_known_name() {
        let uri = Uri::from_static("/.well-known/unknown/value");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::Unsupported(suffix) = result else {
            panic!("not parsed as unsupported")
        };

        assert_eq!(suffix, "unknown");
    }

    #[test]
    fn parse_easy_proxy_dispatch() {
        let uri = Uri::from_static("/.well-known/easy-proxy/http/target.example/8080/path?q=1");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::EasyProxy(protocol, target, target_uri) = result else {
            panic!("not parsed as easy-proxy")
        };

        assert_eq!(protocol, HttpProxySubProtocol::HttpForward);
        assert_eq!(target.host_str(), "target.example");
        assert_eq!(target.port(), 8080);
        assert_eq!(
            target_uri,
            Uri::from_static("http://target.example:8080/path?q=1")
        );
    }

    #[test]
    fn parse_easy_proxy_ipv6_dispatch() {
        let uri = Uri::from_static("/.well-known/easy-proxy/https/2001%3Adb8%3A%3A2/8443/api");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::EasyProxy(protocol, target, target_uri) = result else {
            panic!("not parsed as easy-proxy")
        };

        assert_eq!(protocol, HttpProxySubProtocol::HttpsForward);
        assert_eq!(target.host_str(), "2001:db8::2");
        assert_eq!(target.port(), 8443);
        assert_eq!(
            target_uri,
            Uri::from_static("https://[2001:db8::2]:8443/api")
        );
    }

    #[test]
    fn parse_masque_udp_dispatch() {
        let uri = Uri::from_static("/.well-known/masque/udp/192.0.2.1/53");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::Masque(HttpMasque::Udp(addr)) = result else {
            panic!("not parsed as masque udp")
        };

        assert_eq!(addr.host_str(), "192.0.2.1");
        assert_eq!(addr.port(), 53);
    }

    #[test]
    fn parse_masque_udp_ipv6_dispatch() {
        let uri = Uri::from_static("/.well-known/masque/udp/2001%3Adb8%3A%3A3/53");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::Masque(HttpMasque::Udp(addr)) = result else {
            panic!("not parsed as masque udp")
        };

        assert_eq!(addr.host_str(), "2001:db8::3");
        assert_eq!(addr.port(), 53);
    }

    #[test]
    fn parse_masque_ip_dispatch() {
        let uri = Uri::from_static("/.well-known/masque/ip/example.com/17");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::Masque(HttpMasque::Ip(host, proto)) = result else {
            panic!("not parsed as masque ip")
        };

        assert_eq!(host.unwrap().to_string(), "example.com");
        assert_eq!(proto, Some(17));
    }

    #[test]
    fn parse_masque_ip_ipv6_dispatch() {
        let uri = Uri::from_static("/.well-known/masque/ip/2001%3Adb8%3A%3A4/17");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::Masque(HttpMasque::Ip(host, proto)) = result else {
            panic!("not parsed as masque ip")
        };

        assert_eq!(host.unwrap().to_string(), "2001:db8::4");
        assert_eq!(proto, Some(17));
    }

    #[test]
    fn parse_unsupported_masque_segment() {
        let uri = Uri::from_static("/.well-known/masque/unknown-protocol");
        let result = WellKnownUri::parse(&uri).unwrap().unwrap();
        let WellKnownUri::Unsupported(suffix) = result else {
            panic!("not parsed as unsupported")
        };

        assert_eq!(suffix, "masque/unknown-protocol");
        assert_eq!(
            WellKnownUri::Unsupported(suffix).suffix(),
            "masque/unknown-protocol"
        );
    }

    #[test]
    fn parse_propagates_easy_proxy_error() {
        let uri = Uri::from_static("/.well-known/easy-proxy/");
        let err = WellKnownUri::parse(&uri).unwrap_err();

        assert!(matches!(
            err,
            UriParseError::RequiredFieldNotFound("scheme")
        ));
    }

    #[test]
    fn parse_propagates_masque_error() {
        let uri = Uri::from_static("/.well-known/masque/");
        let err = WellKnownUri::parse(&uri).unwrap_err();

        assert!(matches!(
            err,
            UriParseError::RequiredFieldNotFound("segment")
        ));
    }
}
