/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::IpAddr;

use ahash::AHashMap;
use radix_trie::Trie;

use super::QueryStrategy;
use crate::net::{DomainName, DomainNameParseError, Host};

#[derive(Clone, Eq, PartialEq)]
pub enum ResolveRedirectionValue {
    Domain(DomainName),
    Ip((Vec<IpAddr>, Vec<IpAddr>)),
}

#[derive(Default, Clone, Eq, PartialEq)]
pub struct ResolveRedirectionBuilder {
    ht: AHashMap<DomainName, ResolveRedirectionValue>,
    trie: AHashMap<DomainName, ResolveRedirectionValue>,
}

impl ResolveRedirectionBuilder {
    pub fn insert_exact_addr(&mut self, domain: DomainName, ips: Vec<IpAddr>) {
        let mut ipv4 = Vec::new();
        let mut ipv6 = Vec::new();
        for ip in ips {
            match ip {
                IpAddr::V4(_) => ipv4.push(ip),
                IpAddr::V6(ip6) => {
                    if let Some(ip4) = ip6.to_ipv4_mapped() {
                        ipv4.push(IpAddr::V4(ip4));
                    } else {
                        ipv6.push(ip);
                    }
                }
            }
        }

        self.ht
            .insert(domain, ResolveRedirectionValue::Ip((ipv4, ipv6)));
    }

    pub fn insert_exact_alias(&mut self, domain: DomainName, alias: DomainName) {
        self.ht
            .insert(domain, ResolveRedirectionValue::Domain(alias));
    }

    pub fn insert_parent_alias(&mut self, from: DomainName, to: DomainName) {
        self.trie.insert(from, ResolveRedirectionValue::Domain(to));
    }

    pub fn insert_parent_addr(&mut self, domain: DomainName, ips: Vec<IpAddr>) {
        let mut ipv4 = Vec::new();
        let mut ipv6 = Vec::new();
        for ip in ips {
            match ip {
                IpAddr::V4(_) => ipv4.push(ip),
                IpAddr::V6(ip6) => {
                    if let Some(ip4) = ip6.to_ipv4_mapped() {
                        ipv4.push(IpAddr::V4(ip4));
                    } else {
                        ipv6.push(ip);
                    }
                }
            }
        }

        self.trie
            .insert(domain, ResolveRedirectionValue::Ip((ipv4, ipv6)));
    }

    pub fn build(&self) -> ResolveRedirection {
        let mut trie = Trie::new();
        for (k, v) in self.trie.iter() {
            // append extra '.' to match the exact parent domain
            let lookup_k = k.to_reversed();
            let node = TrieValue {
                from: k.clone(),
                to: v.clone(),
            };
            trie.insert(lookup_k, node);
        }

        ResolveRedirection {
            ht: self.ht.clone(),
            match_trie: !self.trie.is_empty(),
            trie,
        }
    }
}

struct TrieValue {
    from: DomainName,
    to: ResolveRedirectionValue,
}

pub struct ResolveRedirection {
    ht: AHashMap<DomainName, ResolveRedirectionValue>,
    match_trie: bool,
    trie: Trie<String, TrieValue>,
}

impl ResolveRedirection {
    pub fn query_value(
        &self,
        domain: &DomainName,
    ) -> Result<Option<ResolveRedirectionValue>, DomainNameParseError> {
        if !self.ht.is_empty()
            && let Some(v) = self.ht.get(domain)
        {
            return Ok(Some(v.clone()));
        }

        if self.match_trie {
            let reversed_domain = domain.to_reversed();
            if let Some(node) = self.trie.get_ancestor_value(&reversed_domain) {
                match &node.to {
                    ResolveRedirectionValue::Domain(to) => {
                        if let Some(replaced) = domain.replace_suffix(&node.from, to)? {
                            return Ok(Some(ResolveRedirectionValue::Domain(replaced)));
                        }
                    }
                    ResolveRedirectionValue::Ip((ipv4, ipv6)) => {
                        return Ok(Some(ResolveRedirectionValue::Ip((
                            ipv4.clone(),
                            ipv6.clone(),
                        ))));
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn query_first(
        &self,
        domain: &DomainName,
        strategy: QueryStrategy,
    ) -> Result<Option<Host>, DomainNameParseError> {
        if !self.ht.is_empty()
            && let Some(v) = self.ht.get(domain)
        {
            match v {
                ResolveRedirectionValue::Domain(alias) => {
                    return Ok(Some(Host::Domain(alias.clone())));
                }
                ResolveRedirectionValue::Ip((ip4, ip6)) => {
                    if let Some(ip) = strategy.pick_first(ip4, ip6) {
                        return Ok(Some(Host::Ip(ip)));
                    }
                }
            }
        }

        if self.match_trie {
            let reversed_domain = domain.to_reversed();
            if let Some(node) = self.trie.get_ancestor_value(&reversed_domain) {
                match &node.to {
                    ResolveRedirectionValue::Domain(to) => {
                        if let Some(replaced) = domain.replace_suffix(&node.from, to)? {
                            return Ok(Some(Host::Domain(replaced)));
                        }
                    }
                    ResolveRedirectionValue::Ip((ip4, ip6)) => {
                        if let Some(ip) = strategy.pick_first(ip4, ip6) {
                            return Ok(Some(Host::Ip(ip)));
                        }
                    }
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::literal_domain;
    use std::cell::LazyCell;
    use std::str::FromStr;

    const DOMAIN1: LazyCell<DomainName> = LazyCell::new(|| literal_domain!("www.example1.com"));
    const DOMAIN2: LazyCell<DomainName> = LazyCell::new(|| literal_domain!("www.example2.com"));
    const DOMAIN3: LazyCell<DomainName> = LazyCell::new(|| literal_domain!("www.example3.com"));
    const DOMAIN4: LazyCell<DomainName> = LazyCell::new(|| literal_domain!("www.example4.com"));

    #[test]
    fn exact_replace_ips() {
        let mut builder = ResolveRedirectionBuilder::default();
        let ip41 = IpAddr::from_str("1.1.1.1").unwrap();
        let ip42 = IpAddr::from_str("2.2.2.2").unwrap();
        let ip61 = IpAddr::from_str("2001:20::1").unwrap();
        let ip62 = IpAddr::from_str("2001:21::1").unwrap();
        let target_ips1 = vec![ip41, ip42];
        let target_ips2 = vec![ip61, ip62];
        let target_ips3 = vec![ip41, ip42, ip61, ip62];

        builder.insert_exact_addr(DOMAIN1.clone(), target_ips1);
        builder.insert_exact_addr(DOMAIN2.clone(), target_ips2);
        builder.insert_exact_addr(DOMAIN3.clone(), target_ips3);
        let r = builder.build();

        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv4Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv6First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        assert!(
            r.query_first(&DOMAIN1, QueryStrategy::Ipv6Only)
                .unwrap()
                .is_none()
        );

        let ret = r
            .query_first(&DOMAIN2, QueryStrategy::Ipv6Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        let ret = r
            .query_first(&DOMAIN2, QueryStrategy::Ipv6First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        let ret = r
            .query_first(&DOMAIN2, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        assert!(
            r.query_first(&DOMAIN2, QueryStrategy::Ipv4Only)
                .unwrap()
                .is_none()
        );

        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv4Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv6Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv6First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));

        assert!(
            r.query_first(&DOMAIN4, QueryStrategy::Ipv4First)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn exact_replace_alias() {
        let mut builder = ResolveRedirectionBuilder::default();
        let to_domain = literal_domain!("www.1-example.com");
        builder.insert_exact_alias(DOMAIN1.clone(), to_domain.clone());
        let r = builder.build();

        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Domain(to_domain));

        assert!(
            r.query_first(&DOMAIN4, QueryStrategy::Ipv4First)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn parent_replace() {
        let mut builder = ResolveRedirectionBuilder::default();
        builder.insert_parent_alias(literal_domain!("foo.com"), literal_domain!("bar.com"));
        let r = builder.build();

        assert!(
            r.query_first(&literal_domain!("foo.com"), QueryStrategy::Ipv4First,)
                .unwrap()
                .is_none()
        );
        let ret = r
            .query_first(&literal_domain!("a.foo.com"), QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret.to_string(), "a.bar.com");

        assert!(
            r.query_first(&literal_domain!("a.zfoo.com"), QueryStrategy::Ipv4First)
                .unwrap()
                .is_none()
        );
        assert!(
            r.query_first(&literal_domain!("a.fooz.com"), QueryStrategy::Ipv4First)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn parent_replace_ips() {
        let mut builder = ResolveRedirectionBuilder::default();
        let ip41 = IpAddr::from_str("1.1.1.1").unwrap();
        let ip42 = IpAddr::from_str("2.2.2.2").unwrap();
        let ip61 = IpAddr::from_str("2001:20::1").unwrap();
        let ip62 = IpAddr::from_str("2001:21::1").unwrap();
        let target_ips1 = vec![ip41, ip42];
        let target_ips2 = vec![ip61, ip62];
        let target_ips3 = vec![ip41, ip42, ip61, ip62];

        builder.insert_parent_addr(literal_domain!("example1.com"), target_ips1);
        builder.insert_parent_addr(literal_domain!("example2.com"), target_ips2);
        builder.insert_parent_addr(literal_domain!("example3.com"), target_ips3);
        let r = builder.build();

        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv4Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN1, QueryStrategy::Ipv6First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        assert!(
            r.query_first(&DOMAIN1, QueryStrategy::Ipv6Only)
                .unwrap()
                .is_none()
        );

        let ret = r
            .query_first(&DOMAIN2, QueryStrategy::Ipv6Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        let ret = r
            .query_first(&DOMAIN2, QueryStrategy::Ipv6First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        let ret = r
            .query_first(&DOMAIN2, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        assert!(
            r.query_first(&DOMAIN2, QueryStrategy::Ipv4Only)
                .unwrap()
                .is_none()
        );

        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv4Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv4First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip41));
        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv6Only)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));
        let ret = r
            .query_first(&DOMAIN3, QueryStrategy::Ipv6First)
            .unwrap()
            .unwrap();
        assert_eq!(ret, Host::Ip(ip61));

        assert!(
            r.query_first(&DOMAIN4, QueryStrategy::Ipv4First)
                .unwrap()
                .is_none()
        );
    }
}
