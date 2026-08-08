/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_types::metrics::MetricTagMap;

#[derive(Clone, Default)]
pub struct StatsdTagGroup {
    buf: Vec<u8>,
}

impl StatsdTagGroup {
    pub fn add_tag<T: AsRef<str>>(&mut self, key: &str, value: T) {
        if !self.buf.is_empty() {
            self.buf.push(b',');
        }
        self.buf.extend_from_slice(key.as_bytes());
        self.buf.push(b':');
        self.buf.extend_from_slice(value.as_ref().as_bytes());
    }

    pub fn add_static_tags(&mut self, tags: &MetricTagMap) {
        for (k, v) in tags.iter() {
            self.add_tag(k.as_str(), v);
        }
    }

    pub fn add_tag_value<T: AsRef<str>>(&mut self, value: T) {
        if !self.buf.is_empty() {
            self.buf.push(b',');
        }
        self.buf.extend_from_slice(value.as_ref().as_bytes());
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.buf.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vey_types::metrics::MetricTagMap;

    #[test]
    fn add_tag_formats_key_value() {
        let mut tags = StatsdTagGroup::default();
        tags.add_tag("host", "web1");
        tags.add_tag("region", "us");
        assert_eq!(tags.as_bytes(), b"host:web1,region:us");
    }

    #[test]
    fn add_tag_value_appends_bare_values() {
        let mut tags = StatsdTagGroup::default();
        tags.add_tag_value("alpha");
        tags.add_tag_value("beta");
        assert_eq!(tags.as_bytes(), b"alpha,beta");
    }

    #[test]
    fn add_static_tags_from_map() {
        use std::str::FromStr;

        use vey_types::metrics::{MetricTagName, MetricTagValue};

        let mut map = MetricTagMap::default();
        map.insert(
            MetricTagName::from_str("env").unwrap(),
            MetricTagValue::from_str("prod").unwrap(),
        );
        map.insert(
            MetricTagName::from_str("svc").unwrap(),
            MetricTagValue::from_str("proxy").unwrap(),
        );

        let mut tags = StatsdTagGroup::default();
        tags.add_static_tags(&map);
        assert_eq!(tags.as_bytes(), b"env:prod,svc:proxy");
    }
}
