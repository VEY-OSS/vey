/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use serde_json::Value;

use vey_types::acl::{AclAction, AclRegexDomainRuleBuilder};

use super::AclRuleJsonParser;

impl AclRuleJsonParser for AclRegexDomainRuleBuilder {
    #[inline]
    fn get_default_found_action(&self) -> AclAction {
        AclAction::Permit
    }

    #[inline]
    fn set_missed_action(&mut self, _action: AclAction) {
        self.set_missed_action(_action);
    }

    fn add_rule_for_action(&mut self, action: AclAction, value: &Value) -> anyhow::Result<()> {
        match value {
            Value::Object(map) => {
                let mut regex_list = vec![];
                let mut suffix_domain = None;

                for (k, v) in map {
                    match crate::key::normalize(k).as_str() {
                        "regex" => {
                            regex_list = crate::value::as_list(v, crate::value::as_regex)
                                .context(format!("invalid regex list value for key {k}"))?;
                        }
                        "suffix" | "parent" => {
                            let domain = crate::value::as_domain(v)
                                .context(format!("invalid domain string value for key {k}"))?;
                            suffix_domain = Some(domain);
                        }
                        _ => return Err(anyhow!("invalid key {k}")),
                    }
                }

                for regex in regex_list {
                    if let Some(domain) = &suffix_domain {
                        self.add_prefix_regex(domain.clone(), &regex, action);
                    } else {
                        self.add_full_regex(&regex, action);
                    }
                }

                Ok(())
            }
            Value::String(_) => {
                let regex = crate::value::as_regex(value)?;
                self.add_full_regex(&regex, action);
                Ok(())
            }
            _ => Err(anyhow!("invalid value type")),
        }
    }
}

pub(crate) fn as_regex_domain_rule_builder(
    value: &Value,
) -> anyhow::Result<AclRegexDomainRuleBuilder> {
    let mut builder = AclRegexDomainRuleBuilder::new(AclAction::Forbid);
    builder.parse(value)?;
    Ok(builder)
}
