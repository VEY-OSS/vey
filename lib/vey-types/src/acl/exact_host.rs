/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::net::IpAddr;

use super::{AclAHashRule, AclAction, AclFxHashRule, ActionContract};
use crate::net::{DomainName, Host};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AclExactHostRule<Action = AclAction> {
    missed_action: Action,
    domain: AclAHashRule<DomainName, Action>,
    ip: AclFxHashRule<IpAddr, Action>,
}

impl<Action: ActionContract> AclExactHostRule<Action> {
    #[inline]
    pub fn new(missed_action: Action) -> Self {
        AclExactHostRule {
            missed_action,
            domain: AclAHashRule::new(missed_action),
            ip: AclFxHashRule::new(missed_action),
        }
    }

    #[inline]
    pub fn add_domain(&mut self, domain: DomainName, action: Action) {
        self.domain.add_node(domain, action);
    }

    #[inline]
    pub fn add_ip(&mut self, ip: IpAddr, action: Action) {
        self.ip.add_node(ip, action);
    }

    pub fn add_host(&mut self, host: Host, action: Action) {
        match host {
            Host::Ip(ip) => self.add_ip(ip, action),
            Host::Domain(domain) => self.add_domain(domain, action),
        }
    }

    #[inline]
    pub fn set_missed_action(&mut self, action: Action) {
        self.missed_action = action;
        self.domain.set_missed_action(action);
        self.ip.set_missed_action(action);
    }

    #[inline]
    pub fn missed_action(&self) -> Action {
        self.missed_action
    }

    #[inline]
    pub fn check_domain(&self, domain: &DomainName) -> (bool, Action) {
        self.domain.check(domain)
    }

    #[inline]
    pub fn check_ip(&self, ip: &IpAddr) -> (bool, Action) {
        self.ip.check(ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::literal_domain;
    use std::net::Ipv4Addr;
    use std::str::FromStr;

    #[test]
    fn check() {
        let mut rule = AclExactHostRule::new(AclAction::Forbid);
        rule.add_domain(literal_domain!("www.example.com"), AclAction::Permit);
        rule.add_ip(
            IpAddr::from_str("192.168.1.1").unwrap(),
            AclAction::PermitAndLog,
        );

        assert_eq!(
            rule.check_domain(&literal_domain!("www.example.com")),
            (true, AclAction::Permit)
        );
        assert_eq!(
            rule.check_domain(&literal_domain!("www.example.net")),
            (false, AclAction::Forbid)
        );
        assert_eq!(
            rule.check_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))),
            (true, AclAction::PermitAndLog)
        );
        assert_eq!(
            rule.check_ip(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))),
            (false, AclAction::Forbid)
        );

        rule.set_missed_action(AclAction::ForbidAndLog);
        assert_eq!(
            rule.check_domain(&literal_domain!("www.example.net")),
            (false, AclAction::ForbidAndLog)
        );
    }
}
