/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::collections::HashMap;

use radix_trie::{Trie, TrieCommon};
use regex::Regex;

use super::{AclAction, ActionContract, OrderedActionContract, RegexSetBuilder, RegexSetMatch};
use crate::net::DomainName;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclRegexDomainRuleBuilder<Action = AclAction> {
    prefix_regex: HashMap<DomainName, RegexSetBuilder<Action>>,
    full_regex: RegexSetBuilder<Action>,
    missed_action: Action,
}

impl<Action: ActionContract> AclRegexDomainRuleBuilder<Action> {
    pub fn new(missed_action: Action) -> Self {
        AclRegexDomainRuleBuilder {
            prefix_regex: HashMap::new(),
            full_regex: RegexSetBuilder::default(),
            missed_action,
        }
    }

    pub fn add_prefix_regex(&mut self, suffix_domain: DomainName, regex: &Regex, action: Action) {
        self.prefix_regex
            .entry(suffix_domain)
            .or_default()
            .add_regex(regex, action);
    }

    #[inline]
    pub fn add_full_regex(&mut self, regex: &Regex, action: Action) {
        self.full_regex.add_regex(regex, action);
    }

    #[inline]
    pub fn set_missed_action(&mut self, action: Action) {
        self.missed_action = action;
    }

    #[inline]
    pub fn missed_action(&self) -> Action {
        self.missed_action
    }
}

impl<Action: OrderedActionContract> AclRegexDomainRuleBuilder<Action> {
    pub fn build(&self) -> AclRegexDomainRule<Action> {
        let full_match_action_map = self.full_regex.build();

        let mut prefix_match_trie = Trie::new();
        for (suffix, map) in &self.prefix_regex {
            let regex_map = map.build();
            let reversed_k = suffix.to_reversed();
            prefix_match_trie.insert(reversed_k, (suffix.clone(), regex_map));
        }

        AclRegexDomainRule {
            prefix_match_trie,
            full_match_action_map,
            missed_action: self.missed_action,
        }
    }
}

pub struct AclRegexDomainRule<Action = AclAction> {
    prefix_match_trie: Trie<String, (DomainName, RegexSetMatch<Action>)>,
    full_match_action_map: RegexSetMatch<Action>,
    missed_action: Action,
}

impl<Action: ActionContract> AclRegexDomainRule<Action> {
    pub fn check(&self, domain: &DomainName) -> (bool, Action) {
        if !self.prefix_match_trie.is_empty() {
            let reversed = domain.to_reversed();
            if let Some(sub_trie) = self.prefix_match_trie.get_ancestor(&reversed)
                && let Some((suffix, regex_map)) = sub_trie.value()
                && let Some(prefix) = domain.strip_suffix(suffix)
            {
                if let Some(action) = regex_map.check(prefix) {
                    return (true, action);
                }
            }
        }

        if let Some(action) = self.full_match_action_map.check(domain.as_str()) {
            return (true, action);
        }

        (false, self.missed_action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::literal_domain;

    #[test]
    fn check() {
        let mut builder = AclRegexDomainRuleBuilder::new(AclAction::Forbid);

        let regex1 = Regex::new(".*[.]example[.]net$").unwrap();
        builder.add_full_regex(&regex1, AclAction::Permit);

        let regex2 = Regex::new("^www[.]example[.].*").unwrap();
        builder.add_full_regex(&regex2, AclAction::Permit);

        let rule = builder.build();

        assert_eq!(
            rule.check(&literal_domain!("www.example.net")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("www.example.com")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("abc.example.com")),
            (false, AclAction::Forbid)
        );
    }

    #[test]
    fn check_partial() {
        let mut builder = AclRegexDomainRuleBuilder::new(AclAction::Forbid);

        let regex = Regex::new("[.]net$").unwrap();
        builder.add_full_regex(&regex, AclAction::Permit);

        let rule = builder.build();

        assert_eq!(
            rule.check(&literal_domain!("www.example.net")),
            (true, AclAction::Permit)
        );
    }

    #[test]
    fn check_order() {
        let mut builder = AclRegexDomainRuleBuilder::new(AclAction::Forbid);

        let regex = Regex::new("[.]net$").unwrap();
        builder.add_full_regex(&regex, AclAction::Permit);

        let regex = Regex::new("example[.]net$").unwrap();
        builder.add_full_regex(&regex, AclAction::PermitAndLog);

        let regex = Regex::new("f[.]example[.]net$").unwrap();
        builder.add_full_regex(&regex, AclAction::ForbidAndLog);

        let rule = builder.build();

        assert_eq!(
            rule.check(&literal_domain!("www.example.net")),
            (true, AclAction::PermitAndLog)
        );
        assert_eq!(
            rule.check(&literal_domain!("a.example1.net")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("f.example.net")),
            (true, AclAction::ForbidAndLog)
        );
    }

    #[test]
    fn check_prefix() {
        let mut builder = AclRegexDomainRuleBuilder::new(AclAction::Forbid);

        let regex = Regex::new("abc.*$").unwrap();
        builder.add_prefix_regex(literal_domain!("example.net"), &regex, AclAction::Permit);
        let regex = Regex::new("abc.+$").unwrap();
        builder.add_prefix_regex(literal_domain!("example.org"), &regex, AclAction::Permit);

        let rule = builder.build();
        assert_eq!(
            rule.check(&literal_domain!("example.net")),
            (false, AclAction::Forbid)
        );
        assert_eq!(
            rule.check(&literal_domain!("abcd.example.net")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("abc.example.net")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("abcdexample.net")),
            (false, AclAction::Forbid)
        );
        assert_eq!(
            rule.check(&literal_domain!("cde.example.net")),
            (false, AclAction::Forbid)
        );
        assert_eq!(
            rule.check(&literal_domain!("abcd.example.org")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check(&literal_domain!("abc.example.org")),
            (false, AclAction::Forbid)
        );
        assert_eq!(
            rule.check(&literal_domain!("cde.example.org")),
            (false, AclAction::Forbid)
        );
    }
}
