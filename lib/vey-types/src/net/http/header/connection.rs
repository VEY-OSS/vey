/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::time::Duration;

use bytes::BufMut;
use http::{HeaderName, Version};

use super::HttpStructuredFieldParser;
use super::http_names;

/// Parsed HTTP/1.0 `Keep-Alive` header (`timeout` / `max`).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct KeepAliveValue {
    timeout: Option<u64>,
    max: Option<u64>,
}

impl KeepAliveValue {
    #[inline]
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout.map(Duration::from_secs)
    }

    #[inline]
    pub fn max(&self) -> Option<u64> {
        self.max
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.timeout.is_none() && self.max.is_none()
    }

    pub fn parse(&mut self, buf: &[u8]) {
        for item in buf.as_item_list() {
            let param = item.value();
            let Some(eq) = memchr::memchr(b'=', param) else {
                continue;
            };
            let name = param[..eq].trim_ascii();
            let value = param[eq + 1..].trim_ascii();
            if name.is_empty() || value.is_empty() {
                continue;
            }
            let Some(n) = std::str::from_utf8(value).ok().and_then(|s| s.parse().ok()) else {
                continue;
            };
            match name {
                b if b.eq_ignore_ascii_case(b"timeout") => self.timeout = Some(n),
                b if b.eq_ignore_ascii_case(b"max") => self.max = Some(n),
                _ => {}
            }
        }
    }

    pub fn write(&self, name: &[u8], buf: &mut Vec<u8>) {
        if self.is_empty() {
            return;
        }
        buf.put_slice(name);
        buf.put_slice(b": ");
        let mut first = true;
        if let Some(timeout) = self.timeout {
            buf.put_slice(b"timeout=");
            let _ = write!(buf, "{timeout}");
            first = false;
        }
        if let Some(max) = self.max {
            if !first {
                buf.put_slice(b", ");
            }
            buf.put_slice(b"max=");
            let _ = write!(buf, "{max}");
        }
        buf.put_slice(b"\r\n");
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
    keep_alive_name: Option<[u8; 10]>,
    keepalive: KeepAliveValue,
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

    #[inline]
    pub fn keep_alive_header(&self) -> KeepAliveValue {
        self.keepalive
    }

    pub fn clear_keep_alive_header(&mut self) {
        self.keep_alive_name = None;
        self.keepalive = KeepAliveValue::default();
    }

    pub fn parse(&mut self, buf: &[u8]) {
        for item in buf.as_item_list() {
            let token = item.value();
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

    pub fn parse_keep_alive(&mut self, name: &[u8], value: &[u8]) {
        self.keep_alive_name = Some(http_names::copy(name, http_names::KEEP_ALIVE_NAME));
        self.keepalive.parse(value);
    }

    pub fn write_for_rsp(&self, name: &[u8], keep_alive: bool, buf: &mut Vec<u8>) {
        self.write_inner(name, keep_alive, None, buf);
    }

    pub fn write_for_req(
        &self,
        name: &[u8],
        keep_alive: bool,
        te: Option<&[u8]>,
        buf: &mut Vec<u8>,
    ) {
        self.write_inner(name, keep_alive, te, buf);
    }

    fn write_inner(&self, name: &[u8], keep_alive: bool, te: Option<&[u8]>, buf: &mut Vec<u8>) {
        let ka_name = self
            .keep_alive_name
            .as_ref()
            .unwrap_or(&http_names::KEEP_ALIVE_NAME);
        buf.put_slice(name);
        buf.put_slice(b": ");
        if keep_alive {
            buf.put_slice(ka_name);
        } else {
            buf.put_slice(b"Close");
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
        if keep_alive {
            self.keepalive.write(ka_name, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_timeout_and_max() {
        let mut v = KeepAliveValue::default();
        v.parse(b"timeout=5, max=1000");
        assert_eq!(v.timeout(), Some(Duration::from_secs(5)));
        assert_eq!(v.max(), Some(1000));

        v.parse(b"MAX=2, TIMEOUT=15");
        assert_eq!(v.timeout(), Some(Duration::from_secs(15)));
        assert_eq!(v.max(), Some(2));
    }

    #[test]
    fn parse_ignores_unknown_and_invalid() {
        let mut v = KeepAliveValue::default();
        v.parse(b"foo=bar, timeout=abc, max=");
        assert!(v.is_empty());
        v.parse(b"timeout=0");
        assert_eq!(v.timeout(), Some(Duration::ZERO));
    }

    #[test]
    fn write_omits_empty() {
        let mut buf = Vec::new();
        KeepAliveValue::default().write(&http_names::KEEP_ALIVE_NAME, &mut buf);
        assert!(buf.is_empty());

        let mut v = KeepAliveValue::default();
        v.parse(b"timeout=5, max=10");
        v.write(&http_names::KEEP_ALIVE_NAME, &mut buf);
        assert_eq!(buf, b"Keep-Alive: timeout=5, max=10\r\n");
    }

    #[test]
    fn connection_value_parse_and_write() {
        let mut v = ConnectionValue::default();
        v.parse(b"Keep-Alive, TE, upgrade");
        assert!(v.keep_alive(Version::HTTP_10));
        assert!(!v.close(Version::HTTP_10));
        assert!(v.upgrade());

        let mut buf = Vec::new();
        v.write_for_rsp(&http_names::CONNECTION_NAME, true, &mut buf);
        assert_eq!(buf, b"Connection: Keep-Alive, upgrade\r\n");

        buf.clear();
        v.write_for_req(&http_names::CONNECTION_NAME, true, Some(b"TE"), &mut buf);
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
        v.write_for_req(b"connection", false, Some(b"te"), &mut buf);
        assert_eq!(buf, b"connection: Close, te, upgrade, foo\r\n");
    }

    #[test]
    fn write_emits_keep_alive_header_only_when_open() {
        let mut v = ConnectionValue::default();
        v.parse_keep_alive(&http_names::KEEP_ALIVE_NAME, b"timeout=5, max=10");

        let mut buf = Vec::new();
        v.write_for_rsp(&http_names::CONNECTION_NAME, true, &mut buf);
        assert_eq!(
            buf,
            b"Connection: Keep-Alive\r\nKeep-Alive: timeout=5, max=10\r\n"
        );

        buf.clear();
        v.write_for_rsp(&http_names::CONNECTION_NAME, false, &mut buf);
        assert_eq!(buf, b"Connection: Close\r\n");

        let mut v = ConnectionValue::default();
        v.parse_keep_alive(b"keep-alive", b"timeout=5");
        buf.clear();
        v.write_for_rsp(&http_names::CONNECTION_NAME, true, &mut buf);
        assert_eq!(buf, b"Connection: keep-alive\r\nkeep-alive: timeout=5\r\n");
    }
}
