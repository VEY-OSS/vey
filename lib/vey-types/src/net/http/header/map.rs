/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use http::header::{AsHeaderName, Drain, GetAll};
use http::{HeaderMap, HeaderName};

use super::H1HeaderValue;

#[derive(Debug, Default, Clone)]
pub struct H1HeaderMap {
    inner: HeaderMap<H1HeaderValue>,
}

impl H1HeaderMap {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    #[inline]
    pub fn insert(&mut self, name: HeaderName, value: H1HeaderValue) -> Option<H1HeaderValue> {
        self.inner.insert(name, value)
    }

    #[inline]
    pub fn append(&mut self, name: HeaderName, value: H1HeaderValue) {
        self.inner.append(name, value);
    }

    #[inline]
    pub fn remove<K: AsHeaderName>(&mut self, name: K) -> Option<H1HeaderValue> {
        self.inner.remove(name)
    }

    #[inline]
    pub fn contains_key<K: AsHeaderName>(&self, name: K) -> bool {
        self.inner.contains_key(name)
    }

    #[inline]
    pub fn get<K: AsHeaderName>(&self, name: K) -> Option<&H1HeaderValue> {
        self.inner.get(name)
    }

    #[inline]
    pub fn get_mut<K: AsHeaderName>(&mut self, name: K) -> Option<&mut H1HeaderValue> {
        self.inner.get_mut(name)
    }

    #[inline]
    pub fn get_all<K: AsHeaderName>(&self, name: K) -> GetAll<'_, H1HeaderValue> {
        self.inner.get_all(name)
    }

    pub fn for_each<F>(&self, mut call: F)
    where
        F: FnMut(&HeaderName, &H1HeaderValue),
    {
        self.inner
            .iter()
            .for_each(|(name, value)| call(name, value));
    }

    pub fn drain(&mut self) -> Drain<'_, H1HeaderValue> {
        self.inner.drain()
    }
}

impl From<H1HeaderMap> for HeaderMap {
    fn from(mut value: H1HeaderMap) -> Self {
        let mut new_map = HeaderMap::with_capacity(value.inner.capacity());

        let mut last_name: Option<HeaderName> = None;
        for (name, value) in value.inner.drain() {
            match name {
                Some(name) => {
                    last_name = Some(name.clone());
                    new_map.append(name, value.into_inner());
                }
                None => {
                    let Some(name) = &last_name else {
                        break;
                    };
                    new_map.append(name, value.into_inner());
                }
            }
        }
        new_map
    }
}

impl From<&H1HeaderMap> for HeaderMap {
    fn from(value: &H1HeaderMap) -> Self {
        let mut new_map = HeaderMap::with_capacity(value.inner.capacity());
        value.for_each(|name, value| {
            new_map.append(name, value.inner().clone());
        });
        new_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn http_header_map_operations() {
        // creation and is_empty
        let mut map = H1HeaderMap::default();
        assert!(map.is_empty());

        // insert, contains_key, get, and is_empty after insertion
        let name1 = HeaderName::from_static("content-type");
        let value1 = H1HeaderValue::from_static("text/plain");
        assert!(!map.contains_key(&name1));
        assert!(map.insert(name1.clone(), value1.clone()).is_none());
        assert!(map.contains_key(&name1));
        assert!(!map.is_empty());
        assert_eq!(map.get(&name1).unwrap().to_str(), "text/plain");

        // replacing a value
        let value2 = H1HeaderValue::from_static("application/json");
        let old_value = map.insert(name1.clone(), value2).unwrap();
        assert_eq!(old_value.to_str(), "text/plain");
        assert_eq!(map.get(&name1).unwrap().to_str(), "application/json");

        // get_mut
        let mut_ref = map.get_mut(&name1).unwrap();
        mut_ref.set_static_value("text/html");
        assert_eq!(map.get(&name1).unwrap().to_str(), "text/html");

        // append and get_all
        let name2 = HeaderName::from_static("set-cookie");
        let cookie1 = H1HeaderValue::from_static("cookie1=value1");
        let cookie2 = H1HeaderValue::from_static("cookie2=value2");
        map.append(name2.clone(), cookie1);
        map.append(name2.clone(), cookie2);
        let all_cookies: Vec<_> = map.get_all(&name2).iter().map(|v| v.to_str()).collect();
        assert_eq!(all_cookies, vec!["cookie1=value1", "cookie2=value2"]);

        // for_each
        let mut collected_headers = HashMap::new();
        map.for_each(|name, value| {
            collected_headers
                .entry(name.to_string())
                .or_insert_with(Vec::new)
                .push(value.to_str().to_string());
        });
        assert_eq!(collected_headers.len(), 2);
        assert_eq!(
            collected_headers.get("content-type").unwrap(),
            &vec!["text/html"]
        );
        assert_eq!(
            collected_headers.get("set-cookie").unwrap(),
            &vec!["cookie1=value1", "cookie2=value2"]
        );

        // remove
        let removed_value = map.remove(&name1).unwrap();
        assert_eq!(removed_value.to_str(), "text/html");
        assert!(!map.contains_key(&name1));

        // drain
        let mut drained_map = map.clone();
        assert!(!drained_map.is_empty());
        let drained_items: Vec<_> = drained_map.drain().collect();
        assert_eq!(drained_items.len(), 2); // two set-cookie values
        assert!(drained_map.is_empty());

        // From<&H1HeaderMap> for HeaderMap
        let mut map_for_ref_conv = H1HeaderMap::default();
        map_for_ref_conv.insert(
            HeaderName::from_static("x-ref"),
            H1HeaderValue::from_static("ref-value"),
        );
        let header_map_from_ref: HeaderMap = (&map_for_ref_conv).into();
        assert_eq!(header_map_from_ref.get("x-ref").unwrap(), "ref-value");

        // From<H1HeaderMap> for HeaderMap
        let mut map_for_owned_conv = H1HeaderMap::default();
        map_for_owned_conv.insert(
            HeaderName::from_static("x-owned"),
            H1HeaderValue::from_static("owned-value"),
        );
        let header_map_from_owned: HeaderMap = map_for_owned_conv.into();
        assert_eq!(header_map_from_owned.get("x-owned").unwrap(), "owned-value");
    }
}
