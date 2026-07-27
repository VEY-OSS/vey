/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::net::SocketAddr;

use base64::prelude::*;
use bytes::BufMut;

use vey_types::auth::Username;
use vey_types::net::HttpHeaderMap;

pub(crate) fn add_client_addr(buf: &mut Vec<u8>, addr: SocketAddr) {
    let _ = write!(buf, "X-Client-IP: {}\r\n", addr.ip());
    let _ = write!(buf, "X-Client-Port: {}\r\n", addr.port());
}

pub(crate) fn add_client_username(buf: &mut Vec<u8>, user: &str) {
    buf.put_slice(b"X-Client-Username: ");
    let url_encoded = Username::url_encode(user);
    buf.put_slice(url_encoded.as_bytes());
    buf.put_slice(b"\r\n");

    buf.put_slice(b"X-Authenticated-User: ");
    let v = BASE64_STANDARD.encode(format!("Local://{user}"));
    buf.put_slice(v.as_bytes());
    buf.put_slice(b"\r\n");
}

pub(crate) fn add_shared(buf: &mut Vec<u8>, headers: &HttpHeaderMap) {
    headers.for_each(|name, value| {
        buf.put_slice(name.as_str().as_bytes());
        buf.put_slice(b": ");
        buf.put_slice(value.as_bytes());
        buf.put_slice(b"\r\n");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::header::HeaderName;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use vey_types::net::{HttpHeaderMap, HttpHeaderValue};

    #[test]
    fn add_client_addr_serializes_ip_and_port() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8080);
        let mut buf = Vec::new();
        add_client_addr(&mut buf, addr);

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("X-Client-IP: 10.0.0.1\r\n"));
        assert!(text.contains("X-Client-Port: 8080\r\n"));
    }

    #[test]
    fn add_client_username_url_encodes_and_base64_authenticated_user() {
        let mut buf = Vec::new();
        add_client_username(&mut buf, "user@example");

        let text = String::from_utf8(buf).unwrap();
        assert!(text.starts_with("X-Client-Username: "));
        assert!(text.contains("X-Authenticated-User: "));
        assert!(text.contains("user%40example") || text.contains("user@example"));
    }

    #[test]
    fn add_shared_copies_custom_headers() {
        let mut headers = HttpHeaderMap::default();
        headers.append(
            HeaderName::from_static("x-custom"),
            HttpHeaderValue::from_static("alpha"),
        );
        let mut buf = Vec::new();
        add_shared(&mut buf, &headers);

        assert_eq!(String::from_utf8(buf).unwrap(), "x-custom: alpha\r\n");
    }
}
