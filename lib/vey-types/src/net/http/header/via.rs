/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Version};

use super::HttpFieldParser;
use crate::net::{H1HeaderMap, H1HeaderValue, HttpServerId};

/// One RFC 9110 `Via` hop: `received-protocol` plus `received-by`.
///
/// Written as `{protocol} {received_by}` (for example `1.1 edge-1`). Loop
/// detection compares `received-by` against existing `Via` hops.
///
/// `ViaValue<'static>` can be stored and moved independently.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViaValue<'a> {
    protocol: &'static str,
    received_by: Cow<'a, str>,
}

impl<'a> ViaValue<'a> {
    pub fn for_hop(
        version: Version,
        server_id: Option<&'a HttpServerId>,
        fallback_host: impl Into<Cow<'a, str>>,
    ) -> Self {
        let received_by = match server_id {
            Some(id) => Cow::Borrowed(id.as_str()),
            None => fallback_host.into(),
        };
        ViaValue {
            protocol: protocol_for(version),
            received_by,
        }
    }

    #[inline]
    pub fn received_by(&self) -> &str {
        self.received_by.as_ref()
    }

    pub fn into_owned(self) -> ViaValue<'static> {
        ViaValue {
            protocol: self.protocol,
            received_by: Cow::Owned(self.received_by.into_owned()),
        }
    }

    pub fn seen_in_h1(&self, map: &H1HeaderMap) -> bool {
        seen_received_by(
            map.get_all(http::header::VIA).iter().map(|v| v.as_bytes()),
            self.received_by.as_bytes(),
        )
    }

    pub fn seen_in_http(&self, map: &HeaderMap) -> bool {
        seen_received_by(
            map.get_all(http::header::VIA).iter().map(|v| v.as_bytes()),
            self.received_by.as_bytes(),
        )
    }

    pub fn append_to_h1(&self, map: &mut H1HeaderMap) {
        map.append(http::header::VIA, unsafe {
            H1HeaderValue::from_buf_unchecked(self.to_field_value())
        });
    }

    pub fn append_to_http(&self, map: &mut HeaderMap) {
        map.append(http::header::VIA, unsafe {
            HeaderValue::from_maybe_shared_unchecked(Bytes::from(self.to_field_value()))
        });
    }

    fn to_field_value(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(self.protocol.len() + 1 + self.received_by.len());
        buf.extend_from_slice(self.protocol.as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(self.received_by.as_bytes());
        buf
    }
}

fn protocol_for(version: Version) -> &'static str {
    if version == Version::HTTP_09 {
        "0.9"
    } else if version == Version::HTTP_10 {
        "1.0"
    } else if version == Version::HTTP_11 {
        "1.1"
    } else if version == Version::HTTP_2 {
        "2.0"
    } else if version == Version::HTTP_3 {
        "3.0"
    } else {
        "1.1"
    }
}

fn seen_received_by<'a, I>(fields: I, received_by: &[u8]) -> bool
where
    I: IntoIterator<Item = &'a [u8]>,
{
    fields.into_iter().any(|field| {
        field.as_generic_item_list().any(|item| {
            hop_received_by(item.value()).is_some_and(|b| b.eq_ignore_ascii_case(received_by))
        })
    })
}

fn hop_received_by(hop: &[u8]) -> Option<&[u8]> {
    hop.split(|c| c.is_ascii_whitespace())
        .filter(|s| !s.is_empty())
        .nth(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn writes_protocol_version_not_debug_version() {
        let via = ViaValue::for_hop(Version::HTTP_11, None, "edge");
        let mut map = H1HeaderMap::default();
        via.append_to_h1(&mut map);
        assert_eq!(map.get(http::header::VIA).unwrap().to_str(), "1.1 edge");
    }

    #[test]
    fn prefers_server_id() {
        let id = HttpServerId::from_str("gw-1").unwrap();
        let via = ViaValue::for_hop(Version::HTTP_2, Some(&id), "example.com");
        assert_eq!(via.received_by(), "gw-1");
        let mut map = HeaderMap::new();
        via.append_to_http(&mut map);
        assert_eq!(map.get(http::header::VIA).unwrap().as_bytes(), b"2.0 gw-1");
    }

    #[test]
    fn into_owned_is_static() {
        let host = String::from("edge");
        let via = ViaValue::for_hop(Version::HTTP_11, None, host.as_str()).into_owned();
        assert_eq!(via.received_by(), "edge");
    }

    #[test]
    fn detects_loop_in_list_and_own_hop() {
        let via = ViaValue::for_hop(Version::HTTP_11, None, "edge");
        let mut map = H1HeaderMap::default();
        map.append(
            http::header::VIA,
            H1HeaderValue::from_static("1.0 other, 1.1 EDGE"),
        );
        assert!(via.seen_in_h1(&map));

        let mut empty = H1HeaderMap::default();
        assert!(!via.seen_in_h1(&empty));
        via.append_to_h1(&mut empty);
        assert!(via.seen_in_h1(&empty));
    }

    #[test]
    fn skips_comment_after_received_by() {
        let via = ViaValue::for_hop(Version::HTTP_11, None, "proxy.example.com");
        let mut map = HeaderMap::new();
        map.append(
            http::header::VIA,
            HeaderValue::from_static("HTTP/1.1 proxy.example.com:8080 (Apache/1.1)"),
        );
        // received-by includes port; this hop is a different token
        assert!(!via.seen_in_http(&map));
        map.append(
            http::header::VIA,
            HeaderValue::from_static("1.1 proxy.example.com (Apache/1.1)"),
        );
        assert!(via.seen_in_http(&map));
    }
}
