/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 VEY-OSS Developers.
 */

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UsernameParamsConfig {
    required_keys: BTreeSet<String>,
    optional_keys: BTreeSet<String>,
    reject_unknown_keys: bool,
    param_separator: char,
}

impl UsernameParamsConfig {
    pub(crate) fn new() -> Self {
        UsernameParamsConfig {
            required_keys: BTreeSet::new(),
            optional_keys: BTreeSet::new(),
            reject_unknown_keys: true,
            param_separator: '-',
        }
    }
}

impl UsernameParamsConfig {
    pub(crate) fn parse(value: &Yaml) -> anyhow::Result<Self> {
        if let Yaml::Hash(map) = value {
            let mut c = Self::new();
            vey_yaml::foreach_kv(map, |k, v| c.set(k, v))?;
            c.check()?;
            Ok(c)
        } else {
            Err(anyhow!(
                "Yaml value type for `UsernameParamsConfig` should be map"
            ))
        }
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "required_keys" | "required" => {
                let keys = vey_yaml::value::as_list(v, vey_yaml::value::as_string)
                    .context(format!("invalid string list value for key {k}"))?;
                for k in keys {
                    if !k.is_empty() {
                        self.required_keys.insert(k);
                    }
                }
                Ok(())
            }
            "optional_keys" | "optional" => {
                let keys = vey_yaml::value::as_list(v, vey_yaml::value::as_string)
                    .context(format!("invalid string list value for key {k}"))?;
                for k in keys {
                    if !k.is_empty() {
                        self.optional_keys.insert(k);
                    }
                }
                Ok(())
            }
            "reject_unknown_keys" | "reject_unknown" => {
                self.reject_unknown_keys = vey_yaml::value::as_bool(v)?;
                Ok(())
            }
            "param_separator" => {
                self.param_separator = vey_yaml::value::as_char(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        for k in &self.optional_keys {
            if self.required_keys.contains(k) {
                return Err(anyhow!(
                    "optional key {k} must not be listed as required key"
                ));
            }
        }

        Ok(())
    }

    pub(crate) fn parse_name_and_params<'a>(
        &self,
        raw: &'a str,
    ) -> anyhow::Result<(&'a str, BTreeMap<String, String>)> {
        let mut param_start = 0;
        for part in raw.split(self.param_separator) {
            if self.required_keys.contains(part) || self.optional_keys.contains(part) {
                let mut find_part = self.param_separator.to_string();
                find_part.push_str(part);
                param_start = raw.find(&find_part).unwrap_or_default();
                break;
            }
        }

        if param_start == 0 {
            return Err(anyhow!("no username found, all are params"));
        }

        let username = &raw[0..param_start];
        let params = &raw[param_start..];

        let mut context = BTreeMap::new();
        let mut iter = params.split(self.param_separator);
        let _ = iter.next(); // skip the leading separator
        while let Some(name) = iter.next() {
            match iter.next() {
                Some(value) => {
                    if self.required_keys.contains(name) || self.optional_keys.contains(name) {
                        context.insert(name.to_owned(), value.to_owned());
                    } else if self.reject_unknown_keys {
                        return Err(anyhow!("unknown username param {name}: {value}"));
                    }
                }
                None => return Err(anyhow!("no value found for param {name}")),
            }
        }

        for k in &self.required_keys {
            if !context.contains_key(k) {
                return Err(anyhow!("missing required username param {k}"));
            }
        }

        Ok((username, context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vey_yaml::yaml_doc;

    #[test]
    fn t() {
        let doc = yaml_doc! {
            r#"
            required_keys:
              - country
            optional_keys:
              - city
            reject_unknown_keys: true
            param_separator: '-'
            "#
        };
        let config = UsernameParamsConfig::parse(&doc).unwrap();

        let (name, context) = config
            .parse_name_and_params("foo-bar-country-cn-city-none")
            .unwrap();
        assert_eq!(name, "foo-bar");
        let country = context.get("country").unwrap();
        assert_eq!(country, "cn");
        let city = context.get("city").unwrap();
        assert_eq!(city, "none");
    }
}
