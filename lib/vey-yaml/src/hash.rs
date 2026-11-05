/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::hash::BuildHasher;

use anyhow::{Context, anyhow};
use saphyr::{Mapping, Yaml};

fn mapping_get<'a, 'input>(map: &'a Mapping<'input>, key: &str) -> Option<&'a Yaml<'input>> {
    let hash = map.hasher().hash_one(yaml_str!(key));
    map.raw_entry()
        .from_hash(hash, |k| k.as_str() == Some(key))
        .map(|(_, v)| v)
}

pub fn foreach_kv<F>(table: &Mapping, mut f: F) -> anyhow::Result<()>
where
    F: FnMut(&str, &Yaml) -> anyhow::Result<()>,
{
    for (k, v) in table.iter() {
        let Some(key) = k.as_str() else {
            return Err(anyhow!("key in hash should be string"));
        };
        f(key, v).context(format!("failed to parse value of key {key}"))?;
    }
    Ok(())
}

pub fn get_required<'a, 'input>(
    map: &'a Mapping<'input>,
    k: &str,
) -> anyhow::Result<&'a Yaml<'input>> {
    mapping_get(map, k).ok_or_else(|| anyhow!("no required key {k} found in this map"))
}

pub fn get_required_str<'a, 'input>(map: &'a Mapping<'input>, k: &str) -> anyhow::Result<&'a str> {
    get_required(map, k)?
        .as_str()
        .ok_or_else(|| anyhow!("invalid string value for required key {k}"))
}

pub fn get_optional_str<'a, 'input>(
    map: &'a Mapping<'input>,
    k: &str,
) -> anyhow::Result<Option<&'a str>> {
    match mapping_get(map, k) {
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| anyhow!("invalid string value for optional key {k}")),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreach_kv_ok() {
        let yaml = yaml_doc!("a: 1\nb: 2");
        let hash = yaml.as_mapping().unwrap();
        let mut result = Vec::new();
        let res = foreach_kv(hash, |k, v| {
            result.push((k.to_owned(), v.as_integer().unwrap()));
            Ok(())
        });
        assert!(res.is_ok());
        assert_eq!(result, vec![("a".to_string(), 1), ("b".to_string(), 2)]);
    }

    #[test]
    fn foreach_kv_err() {
        let yaml = yaml_doc!("123: 1");
        let hash = yaml.as_mapping().unwrap();
        assert!(foreach_kv(hash, |_, _| Ok(())).is_err());

        let yaml = yaml_doc!("a: 1");
        let hash = yaml.as_mapping().unwrap();
        assert!(foreach_kv(hash, |k, _| Err(anyhow!("error at {}", k))).is_err());
    }

    #[test]
    fn get_required_ok() {
        let yaml = yaml_doc!("key: value");
        let hash = yaml.as_mapping().unwrap();
        assert_eq!(get_required(hash, "key").unwrap(), &yaml_str!("value"));
    }

    #[test]
    fn get_required_err() {
        let yaml = yaml_doc!("key: value");
        let hash = yaml.as_mapping().unwrap();
        assert!(get_required(hash, "missing").is_err());
    }

    #[test]
    fn get_required_str_ok() {
        let yaml = yaml_doc!("key: value");
        let hash = yaml.as_mapping().unwrap();
        assert_eq!(get_required_str(hash, "key").unwrap(), "value");
    }

    #[test]
    fn get_required_str_err() {
        let yaml = yaml_doc!("key: value");
        let hash = yaml.as_mapping().unwrap();
        assert!(get_required_str(hash, "missing").is_err());

        let yaml = yaml_doc!("key: 123");
        let hash = yaml.as_mapping().unwrap();
        assert!(get_required_str(hash, "key").is_err());
    }
}
