/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use http::{HeaderMap, header};

pub const fn connection_as_bytes(close: bool) -> &'static [u8] {
    if close {
        b"Connection: Close\r\n"
    } else {
        b"Connection: Keep-Alive\r\n"
    }
}

/// Remove connection-specific headers which are not allowed in HTTP/2,
/// see <https://datatracker.ietf.org/doc/html/rfc9113#section-8.2.2>.
///
/// Headers listed in the `Connection` header should be removed by the caller.
pub fn remove_h2_connection_specific_headers(headers: &mut HeaderMap) {
    headers.remove(header::CONNECTION);
    headers.remove("keep-alive");
    headers.remove("proxy-connection");
    headers.remove(header::TRANSFER_ENCODING);
    headers.remove(header::UPGRADE);
    if headers
        .get_all(header::TE)
        .iter()
        .any(|v| !v.as_bytes().eq_ignore_ascii_case(b"trailers"))
    {
        headers.remove(header::TE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_connection_as_bytes() {
        assert_eq!(connection_as_bytes(true), b"Connection: Close\r\n");
        assert_eq!(connection_as_bytes(false), b"Connection: Keep-Alive\r\n");
    }

    #[test]
    fn t_remove_h2_connection_specific_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONNECTION, "keep-alive".parse().unwrap());
        headers.insert("keep-alive", "timeout=5".parse().unwrap());
        headers.insert("proxy-connection", "keep-alive".parse().unwrap());
        headers.insert(header::TRANSFER_ENCODING, "chunked".parse().unwrap());
        headers.insert(header::UPGRADE, "websocket".parse().unwrap());
        headers.insert(header::TE, "gzip".parse().unwrap());
        headers.insert(header::CONTENT_TYPE, "text/plain".parse().unwrap());
        remove_h2_connection_specific_headers(&mut headers);
        assert_eq!(headers.len(), 1);
        assert!(headers.contains_key(header::CONTENT_TYPE));

        let mut headers = HeaderMap::new();
        headers.insert(header::TE, "trailers".parse().unwrap());
        remove_h2_connection_specific_headers(&mut headers);
        assert!(headers.contains_key(header::TE));
    }
}
