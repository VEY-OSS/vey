/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::HttpFieldParser;

/// A common header-list item: token (or `name=value`) plus optional raw params.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct GenericItem<'a> {
    value: &'a [u8],
    params: &'a [u8],
}

impl<'a> GenericItem<'a> {
    #[inline]
    pub(super) fn value(&self) -> &'a [u8] {
        self.value
    }

    /// Raw `parameters` string without the first `;`. Empty if the item has none.
    #[inline]
    pub(super) fn params(&self) -> &'a [u8] {
        self.params
    }
}

impl HttpFieldParser for [u8] {
    #[inline]
    fn as_generic_item_list(&self) -> impl Iterator<Item = GenericItem<'_>> {
        GenericItemListIter { rest: self }
    }
}

impl HttpFieldParser for str {
    #[inline]
    fn as_generic_item_list(&self) -> impl Iterator<Item = GenericItem<'_>> {
        self.as_bytes().as_generic_item_list()
    }
}

struct GenericItemListIter<'a> {
    rest: &'a [u8],
}

impl<'a> Iterator for GenericItemListIter<'a> {
    type Item = GenericItem<'a>;

    fn next(&mut self) -> Option<GenericItem<'a>> {
        loop {
            if self.rest.is_empty() {
                return None;
            }
            let (member, rest) = match memchr::memchr(b',', self.rest) {
                Some(i) => (&self.rest[..i], &self.rest[i + 1..]),
                None => (self.rest, [].as_slice()),
            };
            self.rest = rest;
            if let Some(item) = parse_member(member) {
                return Some(item);
            }
        }
    }
}

fn parse_member(member: &[u8]) -> Option<GenericItem<'_>> {
    let member = member.trim_ascii();
    if member.is_empty() {
        return None;
    }
    let (value, params) = match memchr::memchr(b';', member) {
        Some(i) => (member[..i].trim_ascii(), member[i + 1..].trim_ascii()),
        None => (member, [].as_slice()),
    };
    if value.is_empty() {
        None
    } else {
        Some(GenericItem { value, params })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(s: &str) -> Vec<(&[u8], &[u8])> {
        s.as_generic_item_list()
            .map(|i| (i.value(), i.params()))
            .collect()
    }

    #[test]
    fn empty_and_ows() {
        assert!(items("").is_empty());
        assert!(items("   \t").is_empty());
        assert_eq!(items("foo"), [(b"foo".as_slice(), b"".as_slice())]);
        assert_eq!(
            items(" foo , bar "),
            [
                (b"foo".as_slice(), b"".as_slice()),
                (b"bar".as_slice(), b"".as_slice())
            ]
        );
    }

    #[test]
    fn skips_empty_members() {
        assert_eq!(items("chunked,"), [(b"chunked".as_slice(), b"".as_slice())]);
        assert_eq!(
            items("a,,b"),
            [
                (b"a".as_slice(), b"".as_slice()),
                (b"b".as_slice(), b"".as_slice())
            ]
        );
        assert_eq!(items(",a"), [(b"a".as_slice(), b"".as_slice())]);
        assert!(items(";q=1").is_empty());
    }

    #[test]
    fn value_and_params() {
        assert_eq!(
            items("gzip;q=0.8"),
            [(b"gzip".as_slice(), b"q=0.8".as_slice())]
        );
        assert_eq!(
            items("gzip; q=0.8"),
            [(b"gzip".as_slice(), b"q=0.8".as_slice())]
        );
        assert_eq!(
            items("deflate;q=0.5, gzip;q=1, TRAILERS"),
            [
                (b"deflate".as_slice(), b"q=0.5".as_slice()),
                (b"gzip".as_slice(), b"q=1".as_slice()),
                (b"TRAILERS".as_slice(), b"".as_slice())
            ]
        );
        assert_eq!(
            items("timeout=5, max=1000"),
            [
                (b"timeout=5".as_slice(), b"".as_slice()),
                (b"max=1000".as_slice(), b"".as_slice())
            ]
        );
    }
}
