/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

/// RFC 9651 Item: bare-item in serialized form, plus raw `parameters`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SfItem<'a> {
    value: &'a [u8],
    params: &'a [u8],
}

impl<'a> SfItem<'a> {
    #[inline]
    pub fn value(&self) -> &'a [u8] {
        self.value
    }

    /// Raw `parameters` string without the first `;`. Empty if the item has none.
    ///
    /// A later `ParamIter` can walk this string; it is not parsed here.
    #[inline]
    pub fn params(&self) -> &'a [u8] {
        self.params
    }
}

/// Iterator over an RFC 9651 List that contains only Items (no Inner-list).
///
/// Splits on commas that are not inside a quoted string or inner list, discards
/// OWS around separators, and skips empty members so existing header parsers
/// keep accepting a trailing comma. Each member is split into a serialized
/// item value and a raw params string at the first `;` outside quotes.
pub struct ItemListIter<'a> {
    rest: &'a [u8],
}

/// RFC 9651 Structured Fields parser for header field values.
pub trait HttpStructuredFieldParser<'a> {
    fn as_item_list(&self) -> ItemListIter<'a>;
}

impl<'a> HttpStructuredFieldParser<'a> for &'a [u8] {
    #[inline]
    fn as_item_list(&self) -> ItemListIter<'a> {
        ItemListIter { rest: self }
    }
}

impl<'a> Iterator for ItemListIter<'a> {
    type Item = SfItem<'a>;

    fn next(&mut self) -> Option<SfItem<'a>> {
        loop {
            self.rest = skip_ows(self.rest);
            if self.rest.is_empty() {
                return None;
            }
            let (member, rest) = split_member(self.rest);
            self.rest = rest;
            let member = member.trim_ascii();
            if member.is_empty() {
                continue;
            }
            let item = split_item(member);
            if item.value.is_empty() {
                continue;
            }
            return Some(item);
        }
    }
}

fn skip_ows(buf: &[u8]) -> &[u8] {
    match buf.iter().position(|&b| b != b' ' && b != b'\t') {
        Some(n) => &buf[n..],
        None => &[],
    }
}

fn split_member(input: &[u8]) -> (&[u8], &[u8]) {
    let mut in_string = false;
    let mut escape = false;
    let mut depth = 0u32;
    for (i, &b) in input.iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return (&input[..i], &input[i + 1..]),
            _ => {}
        }
    }
    (input, &[])
}

fn split_item(member: &[u8]) -> SfItem<'_> {
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in member.iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b';' => {
                return SfItem {
                    value: member[..i].trim_ascii(),
                    params: member[i + 1..].trim_ascii(),
                };
            }
            _ => {}
        }
    }
    SfItem {
        value: member,
        params: &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(s: &str) -> Vec<(&[u8], &[u8])> {
        s.as_bytes()
            .as_item_list()
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

    #[test]
    fn quoted_string_and_inner_list() {
        assert_eq!(
            items(r#""a, b", token"#),
            [
                (br#""a, b""#.as_slice(), b"".as_slice()),
                (b"token".as_slice(), b"".as_slice())
            ]
        );
        assert_eq!(
            items(r#"a, (b, c), d"#),
            [
                (b"a".as_slice(), b"".as_slice()),
                (b"(b, c)".as_slice(), b"".as_slice()),
                (b"d".as_slice(), b"".as_slice())
            ]
        );
        assert_eq!(
            items(r#""a, \"b", c"#),
            [
                (br#""a, \"b""#.as_slice(), b"".as_slice()),
                (b"c".as_slice(), b"".as_slice())
            ]
        );
        assert_eq!(
            items(r#""a;b";q=1"#),
            [(br#""a;b""#.as_slice(), b"q=1".as_slice())]
        );
    }

    #[test]
    fn unclosed_quote_or_inner_list_takes_the_rest() {
        assert_eq!(
            items(r#""a, b, c"#),
            [(br#""a, b, c"#.as_slice(), b"".as_slice())]
        );
        assert_eq!(
            items(r#"(a, b, c"#),
            [(b"(a, b, c".as_slice(), b"".as_slice())]
        );
    }
}
