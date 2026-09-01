/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::Write;
use std::time::Duration;

use bytes::BufMut;

pub const KEEP_ALIVE_NAME: [u8; 10] = *b"Keep-Alive";

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
        let mut left = buf;
        while let Some(this) = next_comma_item(&mut left) {
            let param = this.trim_ascii();
            if param.is_empty() {
                continue;
            }
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
        KeepAliveValue::default().write(&KEEP_ALIVE_NAME, &mut buf);
        assert!(buf.is_empty());

        let mut v = KeepAliveValue::default();
        v.parse(b"timeout=5, max=10");
        v.write(&KEEP_ALIVE_NAME, &mut buf);
        assert_eq!(buf, b"Keep-Alive: timeout=5, max=10\r\n");
    }
}
