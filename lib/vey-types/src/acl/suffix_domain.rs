/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use super::{AclAction, AclRadixTrieRule, AclRadixTrieRuleBuilder, ActionContract};
use crate::net::DomainName;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclSuffixDomainRuleBuilder<Action = AclAction>(AclRadixTrieRuleBuilder<String, Action>);

impl<Action: ActionContract> AclSuffixDomainRuleBuilder<Action> {
    #[inline]
    pub fn new(missed_action: Action) -> Self {
        AclSuffixDomainRuleBuilder(AclRadixTrieRuleBuilder::new(missed_action))
    }

    #[inline]
    pub fn add_node(&mut self, domain: &DomainName, action: Action) {
        let reversed_k = domain.to_reversed();
        self.0.add_node(reversed_k, action);
    }

    #[inline]
    pub fn set_missed_action(&mut self, action: Action) {
        self.0.set_missed_action(action);
    }

    #[inline]
    pub fn missed_action(&self) -> Action {
        self.0.missed_action()
    }

    #[inline]
    pub fn build(&self) -> AclSuffixDomainRule<Action> {
        AclSuffixDomainRule(self.0.build())
    }
}

pub struct AclSuffixDomainRule<Action = AclAction>(AclRadixTrieRule<String, Action>);

impl<Action: ActionContract> AclSuffixDomainRule<Action> {
    #[inline]
    pub fn check(&self, domain: &DomainName) -> (bool, Action) {
        let reversed = domain.to_reversed();
        self.0.check(&reversed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::literal_domain;

    #[test]
    fn check() {
        let mut builder = AclSuffixDomainRuleBuilder::new(AclAction::Forbid);
        builder.add_node(&literal_domain!("foo.com"), AclAction::Permit);
        let rule = builder.build();

        assert_eq!(
            rule.check(&literal_domain!("foo.com")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("a.foo.com")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("a.fooz.com")),
            (false, AclAction::Forbid)
        );
        assert_eq!(
            rule.check(&literal_domain!("a.zfoo.com")),
            (false, AclAction::Forbid)
        );
    }
}
