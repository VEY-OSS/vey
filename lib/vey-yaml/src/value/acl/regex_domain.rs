/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

use vey_types::acl::{AclAction, AclRegexDomainRuleBuilder};

use super::AclRuleYamlParser;

impl AclRuleYamlParser for AclRegexDomainRuleBuilder {
    #[inline]
    fn get_default_found_action(&self) -> AclAction {
        AclAction::Permit
    }

    #[inline]
    fn set_missed_action(&mut self, action: AclAction) {
        self.set_missed_action(action);
    }

    fn add_rule_for_action(&mut self, action: AclAction, value: &Yaml) -> anyhow::Result<()> {
        match value {
            Yaml::Hash(map) => {
                let mut regex_list = vec![];
                let mut suffix_domain = None;

                crate::foreach_kv(map, |k, v| match crate::key::normalize(k).as_str() {
                    "regex" => {
                        regex_list = crate::value::as_list(v, crate::value::as_regex)
                            .context(format!("invalid regex list value for key {k}"))?;
                        Ok(())
                    }
                    "suffix" | "parent" => {
                        let domain = crate::value::as_domain(v)
                            .context(format!("invalid domain string value for key {k}"))?;
                        suffix_domain = Some(domain);
                        Ok(())
                    }
                    _ => Err(anyhow!("invalid key {k}")),
                })?;

                for regex in regex_list {
                    if let Some(domain) = &suffix_domain {
                        self.add_prefix_regex(domain.clone(), &regex, action);
                    } else {
                        self.add_full_regex(&regex, action);
                    }
                }

                Ok(())
            }
            Yaml::String(_) => {
                let regex = crate::value::as_regex(value)?;
                self.add_full_regex(&regex, action);
                Ok(())
            }
            _ => Err(anyhow!("invalid value type")),
        }
    }
}

pub(crate) fn as_regex_domain_rule_builder(
    value: &Yaml,
) -> anyhow::Result<AclRegexDomainRuleBuilder> {
    let mut builder = AclRegexDomainRuleBuilder::new(AclAction::Forbid);
    builder.parse(value)?;
    Ok(builder)
}
