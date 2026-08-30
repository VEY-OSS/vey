/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use bytes::BufMut;
use http::{HeaderName, Version};

pub const CONNECTION_NAME: [u8; 10] = *b"Connection";

pub const fn connection_as_bytes(close: bool) -> &'static [u8] {
    if close {
        b"Connection: Close\r\n"
    } else {
        b"Connection: Keep-Alive\r\n"
    }
}

fn next_comma_item<'a>(left: &mut &'a [u8]) -> Option<&'a [u8]> {
    if left.is_empty() {
        return None;
    }
    match memchr::memchr(b',', left) {
        Some(p) => {
            let this = &left[..p];
            *left = &left[p + 1..];
            Some(this)
        }
        None => {
            let this = *left;
            *left = &[];
            Some(this)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConnectionPersistence {
    KeepAlive,
    Close,
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct ConnectionValue {
    persistence: Option<ConnectionPersistence>,
    upgrade: bool,
    extra: Vec<HeaderName>,
}

impl ConnectionValue {
    #[inline]
    pub fn keep_alive(&self, version: Version) -> bool {
        match self.persistence {
            Some(ConnectionPersistence::KeepAlive) => true,
            Some(ConnectionPersistence::Close) => false,
            None => version == Version::HTTP_11,
        }
    }

    #[inline]
    pub fn close(&self, version: Version) -> bool {
        !self.keep_alive(version)
    }

    #[inline]
    pub fn upgrade(&self) -> bool {
        self.upgrade
    }

    pub fn parse(&mut self, buf: &[u8]) {
        let mut left = buf;
        while let Some(this) = next_comma_item(&mut left) {
            let token = this.trim_ascii();
            if token.is_empty() {
                continue;
            }
            match token[0] {
                b'K' | b'k' if token.eq_ignore_ascii_case(b"keep-alive") => {
                    self.persistence = Some(ConnectionPersistence::KeepAlive);
                    continue;
                }
                b'C' | b'c' if token.eq_ignore_ascii_case(b"close") => {
                    self.persistence = Some(ConnectionPersistence::Close);
                    continue;
                }
                b'U' | b'u' if token.eq_ignore_ascii_case(b"upgrade") => {
                    self.upgrade = true;
                    continue;
                }
                b'T' | b't' if token.eq_ignore_ascii_case(b"te") => {
                    // Listed only when we actually emit TE (see write).
                    continue;
                }
                _ => {}
            }
            if let Ok(h) = HeaderName::from_bytes(token) {
                self.extra.push(h);
            }
        }
    }

    pub fn write_for_rsp(&self, name: &[u8], close: bool, buf: &mut Vec<u8>) {
        self.write_inner(name, close, None, buf);
    }

    pub fn write_for_req(&self, name: &[u8], close: bool, te: Option<&[u8]>, buf: &mut Vec<u8>) {
        self.write_inner(name, close, te, buf);
    }

    fn write_inner(&self, name: &[u8], close: bool, te: Option<&[u8]>, buf: &mut Vec<u8>) {
        buf.put_slice(name);
        buf.put_slice(b": ");
        if close {
            buf.put_slice(b"Close");
        } else {
            buf.put_slice(b"Keep-Alive");
        }
        if let Some(te) = te {
            buf.put_slice(b", ");
            buf.put_slice(te);
        }
        if self.upgrade {
            buf.put_slice(b", upgrade");
        }
        for h in &self.extra {
            buf.put_slice(b", ");
            buf.put_slice(h.as_str().as_bytes());
        }
        buf.put_slice(b"\r\n");
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
    fn connection_value_parse_and_write() {
        let mut v = ConnectionValue::default();
        v.parse(b"Keep-Alive, TE, upgrade");
        assert!(v.keep_alive(Version::HTTP_10));
        assert!(!v.close(Version::HTTP_10));
        assert!(v.upgrade());

        let mut buf = Vec::new();
        v.write_for_rsp(&CONNECTION_NAME, false, &mut buf);
        assert_eq!(buf, b"Connection: Keep-Alive, upgrade\r\n");

        buf.clear();
        v.write_for_req(&CONNECTION_NAME, false, Some(b"TE"), &mut buf);
        assert_eq!(buf, b"Connection: Keep-Alive, TE, upgrade\r\n");

        v.parse(b"close, keep-alive, Foo");
        assert!(v.keep_alive(Version::HTTP_11));
        assert!(!v.close(Version::HTTP_11));
        v.parse(b"close");
        assert!(v.close(Version::HTTP_11));
        assert!(!v.keep_alive(Version::HTTP_11));
        let empty = ConnectionValue::default();
        assert!(empty.keep_alive(Version::HTTP_11));
        assert!(!empty.keep_alive(Version::HTTP_10));
        buf.clear();
        v.write_for_req(b"connection", true, Some(b"te"), &mut buf);
        assert_eq!(buf, b"connection: Close, te, upgrade, foo\r\n");
    }
}
