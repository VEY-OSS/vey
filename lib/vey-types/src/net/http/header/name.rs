/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Borrow;
use std::ops::Deref;

use smol_str::SmolStr;

#[derive(Debug, Clone)]
pub struct HttpOriginalHeaderName(SmolStr);

impl HttpOriginalHeaderName {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'a> From<&'a str> for HttpOriginalHeaderName {
    fn from(value: &'a str) -> Self {
        HttpOriginalHeaderName(value.into())
    }
}

impl Borrow<str> for HttpOriginalHeaderName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Deref for HttpOriginalHeaderName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

/// A well-known HTTP header, used as the type parameter of [`HttpKnownHeaderName`].
pub trait HttpKnownHeader: Copy + 'static {
    type Bytes: Copy + AsRef<[u8]>;
    const BYTES: Self::Bytes;

    fn copy(name: impl AsRef<[u8]>) -> Self::Bytes;

    fn default_bytes() -> &'static [u8];
}

/// Wire spelling of a well-known header `H`, or `H`'s canonical name if unseen.
///
/// Unlike [`HttpOriginalHeaderName`], this is typed, fixed-size, and falls back
/// to `H` when nothing has been received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpKnownHeaderName<H: HttpKnownHeader> {
    received: Option<H::Bytes>,
}

impl<H: HttpKnownHeader> HttpKnownHeaderName<H> {
    #[inline]
    pub const fn new() -> Self {
        Self { received: None }
    }

    /// Record the wire name. Later calls keep the first received value.
    pub fn receive(&mut self, name: impl AsRef<[u8]>) {
        if self.received.is_none() {
            self.received = Some(H::copy(name));
        }
    }

    /// Mark as received using the associated default name.
    pub fn receive_default(&mut self) {
        if self.received.is_none() {
            self.received = Some(H::BYTES);
        }
    }

    #[inline]
    pub fn received_or_default(mut self) -> Self {
        self.receive_default();
        self
    }

    #[inline]
    pub fn clear(&mut self) {
        self.received = None;
    }

    #[inline]
    pub fn cleared(self) -> Self {
        Self { received: None }
    }

    #[inline]
    pub fn is_received(&self) -> bool {
        self.received.is_some()
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        match &self.received {
            Some(name) => name.as_ref(),
            None => H::default_bytes(),
        }
    }
}

impl<H: HttpKnownHeader> Default for HttpKnownHeaderName<H> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<H: HttpKnownHeader> AsRef<[u8]> for HttpKnownHeaderName<H> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<H: HttpKnownHeader> Deref for HttpKnownHeaderName<H> {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::http_names::CONNECTION;

    #[test]
    fn http_original_header_name_operations() {
        let header_name = HttpOriginalHeaderName::from("Content-Type");
        assert_eq!(header_name.as_str(), "Content-Type");

        let header_name = HttpOriginalHeaderName::from("User-Agent");
        let s: &str = &header_name;
        assert_eq!(s, "User-Agent");

        let header_name = HttpOriginalHeaderName::from("Accept");
        let borrowed: &str = Borrow::<str>::borrow(&header_name);
        assert_eq!(borrowed, "Accept");
    }

    #[test]
    fn known_header_name() {
        let mut name = HttpKnownHeaderName::<CONNECTION>::new();
        assert!(!name.is_received());
        assert_eq!(&*name, b"Connection");

        name.receive("connection");
        assert!(name.is_received());
        assert_eq!(&*name, b"connection");

        name.receive("CONNECTION");
        assert_eq!(&*name, b"connection");

        let mut adapted = name.cleared();
        assert!(!adapted.is_received());
        adapted.receive_default();
        assert!(adapted.is_received());
        assert_eq!(&*adapted, b"Connection");
        assert_eq!(&*name.received_or_default(), b"connection");
    }
}
