/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use bytes::Bytes;
use http::HeaderName;

use super::value::HeaderValueParam;
use crate::net::Host;

pub(super) const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");
const X_FORWARDED_HOST: HeaderName = HeaderName::from_static("x-forwarded-host");
const X_FORWARDED_PROTO: HeaderName = HeaderName::from_static("x-forwarded-proto");
const X_FORWARDED_PORT: HeaderName = HeaderName::from_static("x-forwarded-port");
const X_FORWARDED_BY: HeaderName = HeaderName::from_static("x-forwarded-by");

#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub enum HttpForwardedHeaderType {
    #[default]
    Classic,
    Standard,
    Disable,
}

impl FromStr for HttpForwardedHeaderType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" | "disable" => Ok(HttpForwardedHeaderType::Disable),
            "classic" | "enable" => Ok(HttpForwardedHeaderType::Classic),
            "standard" | "rfc7239" => Ok(HttpForwardedHeaderType::Standard),
            _ => Err(()),
        }
    }
}

impl HttpForwardedHeaderType {
    pub(super) fn for_each_header(self, mut call: impl FnMut(HeaderName)) {
        match self {
            HttpForwardedHeaderType::Classic => {
                call(X_FORWARDED_FOR);
                call(X_FORWARDED_PROTO);
                call(X_FORWARDED_HOST);
                call(X_FORWARDED_PORT);
                call(X_FORWARDED_BY);
            }
            HttpForwardedHeaderType::Standard => {
                call(http::header::FORWARDED);
            }
            HttpForwardedHeaderType::Disable => {}
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardedProto {
    Http,
    Https,
}

impl ForwardedProto {
    pub const fn from_https(https: bool) -> Self {
        if https {
            ForwardedProto::Https
        } else {
            ForwardedProto::Http
        }
    }

    fn as_bytes(self) -> &'static [u8] {
        match self {
            ForwardedProto::Http => b"http",
            ForwardedProto::Https => b"https",
        }
    }
}

/// This hop's originating-client identity for `Forwarded` / `X-Forwarded-*`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForwardedValue {
    ip: IpAddr,
    port: u16,
    proto: Option<ForwardedProto>,
    host: Host,
    by: Option<SocketAddr>,
}

impl ForwardedValue {
    pub fn from_client(client: SocketAddr, proto: ForwardedProto, host: Host) -> Self {
        ForwardedValue {
            ip: client.ip(),
            port: client.port(),
            proto: Some(proto),
            host,
            by: None,
        }
    }

    pub fn with_by(mut self, by: SocketAddr) -> Self {
        self.by = Some(by);
        self
    }

    pub(super) fn for_each_classic(&self, mut call: impl FnMut(HeaderName, Bytes)) {
        call(X_FORWARDED_FOR, Bytes::from(self.ip.to_string()));
        if let Some(proto) = self.proto {
            call(X_FORWARDED_PROTO, Bytes::from_static(proto.as_bytes()));
        }
        call(X_FORWARDED_HOST, Bytes::from(self.host.to_string()));
        if self.port != 0 {
            call(X_FORWARDED_PORT, Bytes::from(self.port.to_string()));
        }
        if let Some(by) = self.by {
            call(X_FORWARDED_BY, Bytes::from(by.to_string()));
        }
    }

    pub(super) fn standard_field(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(b"for=");
        HeaderValueParam::serialize_node(&mut buf, self.ip, self.port);
        if let Some(by) = self.by {
            buf.extend_from_slice(b"; by=");
            HeaderValueParam::serialize_node(&mut buf, by.ip(), by.port());
        }
        if let Some(proto) = self.proto {
            buf.extend_from_slice(b"; proto=");
            buf.extend_from_slice(proto.as_bytes());
        }
        buf.extend_from_slice(b"; host=");
        HeaderValueParam::serialize_host(&mut buf, &self.host);
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use http::{HeaderMap, HeaderValue};

    use super::super::HeaderMapExt;
    use crate::net::Host;

    #[test]
    fn http_forwarded_header_type_operations() {
        // valid cases
        assert_eq!(
            "none".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Disable
        );
        assert_eq!(
            "disable".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Disable
        );
        assert_eq!(
            "classic".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Classic
        );
        assert_eq!(
            "enable".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Classic
        );
        assert_eq!(
            "standard".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Standard
        );
        assert_eq!(
            "rfc7239".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Standard
        );

        // case insensitivity
        assert_eq!(
            "NONE".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Disable
        );
        assert_eq!(
            "DISABLE".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Disable
        );
        assert_eq!(
            "CLASSIC".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Classic
        );
        assert_eq!(
            "ENABLE".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Classic
        );
        assert_eq!(
            "STANDARD".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Standard
        );
        assert_eq!(
            "RFC7239".parse::<HttpForwardedHeaderType>().unwrap(),
            HttpForwardedHeaderType::Standard
        );

        // invalid cases
        assert!("invalid".parse::<HttpForwardedHeaderType>().is_err());
        assert!("".parse::<HttpForwardedHeaderType>().is_err());
        assert!("unknown".parse::<HttpForwardedHeaderType>().is_err());

        // default value
        assert_eq!(
            HttpForwardedHeaderType::default(),
            HttpForwardedHeaderType::Classic
        );
    }

    fn forwarded_map() -> HeaderMap {
        let mut map = HeaderMap::new();
        map.append("x-forwarded-for", HeaderValue::from_static("203.0.113.9"));
        map.append(
            "forwarded",
            HeaderValue::from_static("for=192.0.2.8; proto=https; host=app.example"),
        );
        map.append("x-forwarded-proto", HeaderValue::from_static("https"));
        map.append("x-forwarded-host", HeaderValue::from_static("xff.example"));
        map.append("x-forwarded-port", HeaderValue::from_static("8443"));
        map.append("x-forwarded-by", HeaderValue::from_static("10.0.0.9"));
        map.append("host", HeaderValue::from_static("keep.example"));
        map
    }

    fn this_hop() -> ForwardedValue {
        let client = "192.0.2.50:1234".parse().unwrap();
        let host = Host::from_str("app.example").unwrap();
        ForwardedValue::from_client(client, ForwardedProto::Http, host)
            .with_by("10.0.0.2:80".parse().unwrap())
    }

    #[test]
    fn untrusted_strips_inbound_then_appends_this_hop() {
        let mut map = forwarded_map();
        let v = this_hop();
        map.strip_forwarded(HttpForwardedHeaderType::Classic);
        map.append_forwarded(&v, HttpForwardedHeaderType::Classic);
        let xff: Vec<_> = map
            .get_all("x-forwarded-for")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(xff, ["192.0.2.50"]);
        assert_eq!(map.get("x-forwarded-proto").unwrap(), "http");
        assert_eq!(map.get("x-forwarded-host").unwrap(), "app.example");
        assert_eq!(map.get("x-forwarded-port").unwrap(), "1234");
        assert_eq!(
            map.get(http::header::FORWARDED).unwrap(),
            "for=192.0.2.8; proto=https; host=app.example"
        );
        assert_eq!(map.get("x-forwarded-by").unwrap(), "10.0.0.2:80");
        assert_eq!(map.get("host").unwrap(), "keep.example");

        let mut map = forwarded_map();
        map.strip_forwarded(HttpForwardedHeaderType::Standard);
        map.append_forwarded(&v, HttpForwardedHeaderType::Standard);
        assert_eq!(
            map.get(http::header::FORWARDED).unwrap().as_bytes(),
            br#"for="192.0.2.50:1234"; by="10.0.0.2:80"; proto=http; host=app.example"#
        );
        assert_eq!(map.get("x-forwarded-for").unwrap(), "203.0.113.9");
        assert_eq!(map.get("x-forwarded-by").unwrap(), "10.0.0.9");
    }

    #[test]
    fn disable_neither_strips_nor_appends() {
        let mut map = forwarded_map();
        let v = this_hop();
        map.strip_forwarded(HttpForwardedHeaderType::Disable);
        map.append_forwarded(&v, HttpForwardedHeaderType::Disable);
        assert_eq!(map.get("x-forwarded-for").unwrap(), "203.0.113.9");
        assert_eq!(
            map.get(http::header::FORWARDED).unwrap(),
            "for=192.0.2.8; proto=https; host=app.example"
        );
        assert_eq!(map.get("x-forwarded-proto").unwrap(), "https");
        assert_eq!(map.get("x-forwarded-host").unwrap(), "xff.example");
        assert_eq!(map.get("x-forwarded-port").unwrap(), "8443");
        assert_eq!(map.get("x-forwarded-by").unwrap(), "10.0.0.9");
        assert_eq!(map.get_all("x-forwarded-for").iter().count(), 1);
    }

    #[test]
    fn trusted_keeps_inbound_then_appends_this_hop() {
        let mut map = forwarded_map();
        let v = this_hop();
        map.append_forwarded(&v, HttpForwardedHeaderType::Classic);
        let xff: Vec<_> = map
            .get_all("x-forwarded-for")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(xff, ["203.0.113.9", "192.0.2.50"]);
        assert!(
            map.get_all("x-forwarded-proto")
                .iter()
                .any(|v| v == "https")
        );
        assert!(map.get_all("x-forwarded-proto").iter().any(|v| v == "http"));
        assert!(map.contains_key(http::header::FORWARDED));
        let xfb: Vec<_> = map
            .get_all("x-forwarded-by")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(xfb, ["10.0.0.9", "10.0.0.2:80"]);
        assert_eq!(map.get("host").unwrap(), "keep.example");
    }

    #[test]
    fn standard_ipv6_is_quoted_and_bracketed() {
        let client = "[2001:db8::1]:443".parse().unwrap();
        let host = Host::from_str("2001:db8::8").unwrap();
        let v = ForwardedValue::from_client(client, ForwardedProto::Https, host)
            .with_by("[2001:db8::2]:8080".parse().unwrap());
        let mut map = HeaderMap::new();
        map.append_forwarded(&v, HttpForwardedHeaderType::Standard);
        assert_eq!(
            map.get(http::header::FORWARDED).unwrap().as_bytes(),
            br#"for="[2001:db8::1]:443"; by="[2001:db8::2]:8080"; proto=https; host="[2001:db8::8]""#
        );
    }
}
