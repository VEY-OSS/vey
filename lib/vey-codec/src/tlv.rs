/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

pub trait TlvParse<'a> {
    const TAG_SIZE: usize;
    const LENGTH_SIZE: usize;
    type Tag;
    type Error;

    fn tag(buf: &[u8]) -> Self::Tag;
    fn length(buf: &[u8]) -> usize;
    fn no_enough_data() -> Self::Error;
    fn parse_value(&mut self, tag: Self::Tag, buf: &'a [u8]) -> Result<(), Self::Error>;

    fn parse_tlv(&mut self, v: &'a [u8]) -> Result<(), Self::Error> {
        let total_len = v.len();
        let mut offset = 0usize;

        loop {
            if offset + Self::TAG_SIZE + Self::LENGTH_SIZE > total_len {
                return Err(Self::no_enough_data());
            }

            let buf = &v[offset..];
            let tag = Self::tag(&buf[0..Self::TAG_SIZE]);
            let vl = Self::length(&buf[Self::TAG_SIZE..]);
            offset += Self::TAG_SIZE + Self::LENGTH_SIZE;
            if offset + vl > total_len {
                return Err(Self::no_enough_data());
            }

            let buf = &v[offset..offset + vl];
            self.parse_value(tag, buf)?;
            offset += vl;
            if offset == total_len {
                return Ok(());
            }
        }
    }
}

pub trait T1L2BVParse<'a> {
    type Error;

    fn no_enough_data() -> Self::Error;
    fn parse_value(&mut self, tag: u8, buf: &'a [u8]) -> Result<(), Self::Error>;
}

impl<'a, T> TlvParse<'a> for T
where
    T: T1L2BVParse<'a>,
{
    const TAG_SIZE: usize = 1;
    const LENGTH_SIZE: usize = 2;
    type Tag = u8;
    type Error = T::Error;

    fn tag(buf: &[u8]) -> Self::Tag {
        buf[0]
    }

    fn length(buf: &[u8]) -> usize {
        u16::from_be_bytes([buf[0], buf[1]]) as usize
    }

    fn no_enough_data() -> Self::Error {
        T::no_enough_data()
    }

    fn parse_value(&mut self, tag: Self::Tag, buf: &'a [u8]) -> Result<(), Self::Error> {
        self.parse_value(tag, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::{T1L2BVParse, TlvParse};

    #[derive(Default)]
    struct Collector {
        values: Vec<(u8, Vec<u8>)>,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Err {
        Truncated,
        BadTag,
    }

    impl<'a> T1L2BVParse<'a> for Collector {
        type Error = Err;

        fn no_enough_data() -> Self::Error {
            Err::Truncated
        }

        fn parse_value(&mut self, tag: u8, buf: &'a [u8]) -> Result<(), Self::Error> {
            if tag == 0xFF {
                return Err(Err::BadTag);
            }
            self.values.push((tag, buf.to_vec()));
            Ok(())
        }
    }

    #[test]
    fn parse_t1l2_ok() {
        // tag=1, len=2, value=ab ; tag=2, len=0
        let data = [0x01, 0x00, 0x02, b'a', b'b', 0x02, 0x00, 0x00];
        let mut c = Collector::default();
        c.parse_tlv(&data).unwrap();
        assert_eq!(c.values, vec![(1, b"ab".to_vec()), (2, Vec::new())]);
    }

    #[test]
    fn parse_t1l2_truncated() {
        let mut c = Collector::default();
        assert_eq!(c.parse_tlv(&[0x01, 0x00]).unwrap_err(), Err::Truncated);
        assert_eq!(
            c.parse_tlv(&[0x01, 0x00, 0x02, b'a']).unwrap_err(),
            Err::Truncated
        );
    }

    #[test]
    fn parse_t1l2_propagates_value_error() {
        let data = [0xFF, 0x00, 0x01, 0x00];
        let mut c = Collector::default();
        assert_eq!(c.parse_tlv(&data).unwrap_err(), Err::BadTag);
    }
}
