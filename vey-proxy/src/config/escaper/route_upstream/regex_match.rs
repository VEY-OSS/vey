/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use anyhow::{Context, anyhow};
use radix_trie::{Trie, TrieCommon};
use regex::RegexSet;
use yaml_rust::Yaml;

use crate::config::escaper::verify::EscaperConfigVerifier;

use vey_types::metrics::NodeName;
use vey_types::net::DomainName;

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct RegexMatchBuilder {
    inner: BTreeMap<NodeName, BTreeSet<RegexMatchValue>>,
}

impl RegexMatchBuilder {
    pub(super) fn check(&self) -> anyhow::Result<()> {
        EscaperConfigVerifier::check_duplicated_rule(&self.inner)
            .context("found duplicated rule for regex match")?;
        Ok(())
    }

    pub(super) fn collect_escaper(&self, set: &mut BTreeSet<NodeName>) {
        set.extend(self.inner.keys().cloned())
    }

    pub(super) fn set_by_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        match value {
            Yaml::Hash(map) => vey_yaml::foreach_kv(map, |k, v| {
                let escaper = NodeName::from_str(k)
                    .map_err(|e| anyhow!("the map key is not valid escaper name: {e}"))?;
                let regexes = vey_yaml::value::as_list(v, RegexMatchValue::parse_yaml)
                    .context(format!("invalid regex match rule values for key {k}"))?;
                self.add_rule(escaper, regexes);
                Ok(())
            }),
            Yaml::Array(seq) => {
                for (i, v) in seq.iter().enumerate() {
                    let Yaml::Hash(map) = v else {
                        return Err(anyhow!("yaml value type for #{i} should be map"));
                    };

                    let mut escaper = NodeName::default();
                    let mut regexes = Vec::new();
                    vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                        "next" | "escaper" => {
                            escaper = vey_yaml::value::as_metric_node_name(v)?;
                            Ok(())
                        }
                        "rules" | "rule" => {
                            regexes = vey_yaml::value::as_list(v, RegexMatchValue::parse_yaml)?;
                            Ok(())
                        }
                        _ => Err(anyhow!("invalid key {k}")),
                    })
                    .context(format!("invalid suffix match rule for #{i}"))?;

                    self.add_rule(escaper, regexes);
                }
                Ok(())
            }
            _ => Err(anyhow!("suffix match rules should be a map or an array")),
        }
    }

    fn add_rule(&mut self, escaper: NodeName, regexes: Vec<RegexMatchValue>) {
        self.inner.entry(escaper).or_default().extend(regexes);
    }

    pub(crate) fn build<T: Clone>(
        &self,
        value_table: &BTreeMap<NodeName, T>,
    ) -> anyhow::Result<Option<RegexMatch<T>>> {
        if self.inner.is_empty() {
            return Ok(None);
        }

        let mut suffix_match_map: BTreeMap<DomainName, Vec<(RegexSet, T)>> = BTreeMap::new();
        let mut full_match_vec = Vec::new();
        for (escaper, rules) in &self.inner {
            let mut suffix_regex_map: BTreeMap<DomainName, Vec<&str>> = BTreeMap::new();
            let mut full_regex_set: BTreeSet<&str> = BTreeSet::new();
            for rule in rules {
                match &rule.suffix_domain {
                    Some(domain) => {
                        suffix_regex_map
                            .entry(domain.clone())
                            .or_default()
                            .push(&rule.sub_domain_regex);
                    }
                    None => {
                        full_regex_set.insert(&rule.sub_domain_regex);
                    }
                }
            }

            let Some(value) = value_table.get(escaper) else {
                return Err(anyhow!("no regex match value found for escaper {escaper}"));
            };
            for (suffix_domain, regexes) in suffix_regex_map {
                let regex_set = RegexSet::new(regexes)
                    .map_err(|e| anyhow!("failed to build regex for escaper {escaper}: {e}"))?;
                suffix_match_map
                    .entry(suffix_domain)
                    .or_default()
                    .push((regex_set, value.clone()));
            }
            if !full_regex_set.is_empty() {
                let regex_set = RegexSet::new(full_regex_set).unwrap();
                full_match_vec.push((regex_set, value.clone()));
            }
        }
        let mut suffix_match_trie = Trie::new();
        for (suffix_domain, value) in suffix_match_map {
            let reversed_k = suffix_domain.to_reversed();
            suffix_match_trie.insert(reversed_k, (suffix_domain, value));
        }
        if suffix_match_trie.is_empty() {
            Ok(None)
        } else {
            Ok(Some(RegexMatch {
                suffix_match: suffix_match_trie,
                full_match: full_match_vec,
            }))
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RegexMatchValue {
    suffix_domain: Option<DomainName>,
    sub_domain_regex: String,
}

impl fmt::Display for RegexMatchValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(domain) = &self.suffix_domain {
            write!(
                f,
                "regex {} for suffix domain {domain}",
                self.sub_domain_regex
            )
        } else {
            write!(f, "regex {} for all domains", self.sub_domain_regex)
        }
    }
}

impl RegexMatchValue {
    fn parse_yaml(value: &Yaml) -> anyhow::Result<Self> {
        let mut match_value = RegexMatchValue::default();
        match value {
            Yaml::Hash(map) => {
                vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
                    "suffix" | "parent" => {
                        let domain = vey_yaml::value::as_domain(v)?;
                        match_value.suffix_domain = Some(domain);
                        Ok(())
                    }
                    "regex" => {
                        let regex = vey_yaml::value::as_regex(v)?;
                        match_value.sub_domain_regex = regex.to_string();
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {k}")),
                })?
            }
            Yaml::String(_) => {
                let regex = vey_yaml::value::as_regex(value)?;
                match_value.sub_domain_regex = regex.to_string();
            }
            _ => {
                return Err(anyhow!("invalid value type for regex match rule value"));
            }
        }
        if match_value.sub_domain_regex.is_empty() {
            return Err(anyhow!("no regular expression set"));
        }
        Ok(match_value)
    }
}

pub(crate) struct RegexMatch<T> {
    suffix_match: Trie<String, (DomainName, Vec<(RegexSet, T)>)>,
    full_match: Vec<(RegexSet, T)>,
}

impl<T> RegexMatch<T> {
    pub(crate) fn check_domain(&self, domain: &DomainName) -> Option<&T> {
        let reversed = domain.to_reversed();
        if let Some(sub_trie) = self.suffix_match.get_ancestor(&reversed)
            && let Some((suffix, rules)) = sub_trie.value()
            && let Some(prefix) = domain.strip_suffix(suffix)
        {
            for (regex, value) in rules {
                if regex.is_match(prefix) {
                    return Some(value);
                }
            }
        }
        for (regex_set, value) in &self.full_match {
            if regex_set.is_match(domain.as_str()) {
                return Some(value);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vey_types::literal_domain;
    use yaml_rust::YamlLoader;

    #[test]
    fn yaml_seq() {
        let conf = r#"
        - next: escaper_1
          rule:
            suffix: example.net
            regex: abc.*
        - next: escaper_2
          rules:
            - suffix: example.net
              regex: cde.+
            - .*[.]example[.]org
        "#;

        let v = YamlLoader::load_from_str(conf).unwrap();
        let mut builder = RegexMatchBuilder::default();
        builder.set_by_yaml(&v[0]).unwrap();

        let mut value_map = BTreeMap::new();
        value_map.insert(unsafe { NodeName::new_unchecked("escaper_1") }, "escaper_1");
        value_map.insert(unsafe { NodeName::new_unchecked("escaper_2") }, "escaper_2");
        let regex_match = builder.build(&value_map).unwrap().unwrap();

        assert!(
            regex_match
                .check_domain(&literal_domain!("example.net"))
                .is_none()
        );
        let value = *regex_match
            .check_domain(&literal_domain!("abc.example.net"))
            .unwrap();
        assert!(value.eq("escaper_1"));
        assert!(
            regex_match
                .check_domain(&literal_domain!("abcexample.net"))
                .is_none()
        );
        let value = *regex_match
            .check_domain(&literal_domain!("cde1.example.net"))
            .unwrap();
        assert!(value.eq("escaper_2"));
        assert!(
            regex_match
                .check_domain(&literal_domain!("cde.example.net"))
                .is_none()
        );
        let value = *regex_match
            .check_domain(&literal_domain!("a.example.org"))
            .unwrap();
        assert!(value.eq("escaper_2"));
    }

    #[test]
    fn yaml_map() {
        let conf = r#"
        escaper_1:
          suffix: example.net
          regex: abc.*
        escaper_2:
          - suffix: example.net
            regex: cde.+
          - .*[.]example[.]org
        "#;

        let v = YamlLoader::load_from_str(conf).unwrap();
        let mut builder = RegexMatchBuilder::default();
        builder.set_by_yaml(&v[0]).unwrap();

        let mut value_map = BTreeMap::new();
        value_map.insert(unsafe { NodeName::new_unchecked("escaper_1") }, "escaper_1");
        value_map.insert(unsafe { NodeName::new_unchecked("escaper_2") }, "escaper_2");
        let regex_match = builder.build(&value_map).unwrap().unwrap();

        assert!(
            regex_match
                .check_domain(&literal_domain!("example.net"))
                .is_none()
        );
        let value = *regex_match
            .check_domain(&literal_domain!("abc.example.net"))
            .unwrap();
        assert!(value.eq("escaper_1"));
        assert!(
            regex_match
                .check_domain(&literal_domain!("abcexample.net"))
                .is_none()
        );
        let value = *regex_match
            .check_domain(&literal_domain!("cde1.example.net"))
            .unwrap();
        assert!(value.eq("escaper_2"));
        assert!(
            regex_match
                .check_domain(&literal_domain!("cde.example.net"))
                .is_none()
        );
        let value = *regex_match
            .check_domain(&literal_domain!("a.example.org"))
            .unwrap();
        assert!(value.eq("escaper_2"));
    }
}
