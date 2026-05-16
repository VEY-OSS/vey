/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use yaml_rust::Yaml;

use vey_types::acl::{AclAction, AclSuffixDomainRuleBuilder};

use super::AclRuleYamlParser;

impl AclRuleYamlParser for AclSuffixDomainRuleBuilder {
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
            Yaml::String(_) => {
                let host = crate::value::as_domain(value)?;
                self.add_node(&host, action);
                Ok(())
            }
            _ => Err(anyhow!("invalid value type")),
        }
    }
}

pub(crate) fn as_suffix_domain_rule_builder(
    value: &Yaml,
) -> anyhow::Result<AclSuffixDomainRuleBuilder> {
    let mut builder = AclSuffixDomainRuleBuilder::new(AclAction::Forbid);
    builder.parse(value)?;
    Ok(builder)
}
