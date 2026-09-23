/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vey_types::net::DomainName;

use crate::config::escaper::PeerHealthCheckConfig;

struct Failure {
    count: u32,
    until: Instant,
}

/// Failures for one domain. Each domain has its own lock.
pub(super) struct PeerHealth {
    strategy: PeerHealthCheckConfig,
    inner: Mutex<HashMap<SocketAddr, Failure>>,
}

impl PeerHealth {
    fn new(strategy: PeerHealthCheckConfig) -> Arc<Self> {
        Arc::new(PeerHealth {
            strategy,
            inner: Mutex::new(HashMap::new()),
        })
    }

    /// Move addresses this escaper recently failed to connect to where `pop()` tries them last.
    /// Relative order inside each group stays as the resolver returned it.
    pub(super) fn reorder(&self, port: u16, ips: &mut Vec<IpAddr>) {
        if ips.len() < 2 {
            return;
        }
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        let unhealthy: Vec<bool> = ips
            .iter()
            .map(|ip| self.is_unhealthy(&mut inner, SocketAddr::new(*ip, port), now))
            .collect();
        drop(inner);
        partition_unhealthy_first(ips, &unhealthy);
    }

    pub(super) fn clear_failure(&self, peer: SocketAddr) {
        self.inner.lock().unwrap().remove(&peer);
    }

    pub(super) fn record_failure(&self, peer: SocketAddr) {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        let entry = inner.entry(peer).or_insert(Failure {
            count: 0,
            until: now,
        });
        if entry.until <= now {
            entry.count = 1;
            entry.until = now + self.strategy.fail_timeout;
        } else {
            entry.count = entry.count.saturating_add(1);
            if entry.count >= self.strategy.max_fails {
                entry.until = now + self.strategy.fail_timeout;
            }
        }
    }

    /// Drop addresses that recently failed. If every address failed, keep the full list.
    pub(super) fn skip_recent_failures(&self, port: u16, ips: Vec<IpAddr>) -> Vec<IpAddr> {
        if ips.len() < 2 {
            return ips;
        }
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        let healthy: Vec<IpAddr> = ips
            .iter()
            .copied()
            .filter(|ip| !self.is_unhealthy(&mut inner, SocketAddr::new(*ip, port), now))
            .collect();
        if healthy.is_empty() { ips } else { healthy }
    }

    fn sweep_expired(&self) {
        let now = Instant::now();
        self.inner
            .lock()
            .unwrap()
            .retain(|_, failure| failure.until > now);
    }

    fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    fn is_unhealthy(
        &self,
        inner: &mut HashMap<SocketAddr, Failure>,
        peer: SocketAddr,
        now: Instant,
    ) -> bool {
        match inner
            .get(&peer)
            .map(|failure| (failure.count, failure.until))
        {
            Some((count, until)) if until > now && count >= self.strategy.max_fails => true,
            Some((_, until)) if until <= now => {
                inner.remove(&peer);
                false
            }
            _ => false,
        }
    }
}

/// One domain in [`PeerHealthTable`]. Looking a domain up does not insert it.
pub(super) struct ScopedPeerHealth {
    table: Arc<PeerHealthTable>,
    domain: DomainName,
}

impl ScopedPeerHealth {
    pub(super) fn reorder(&self, port: u16, ips: &mut Vec<IpAddr>) {
        let Some(health) = self.table.lookup(&self.domain) else {
            return;
        };
        health.sweep_expired();
        health.reorder(port, ips);
        self.table.drop_if_empty(&self.domain, &health);
    }

    pub(super) fn skip_recent_failures(&self, port: u16, ips: Vec<IpAddr>) -> Vec<IpAddr> {
        let Some(health) = self.table.lookup(&self.domain) else {
            return ips;
        };
        health.sweep_expired();
        let ips = health.skip_recent_failures(port, ips);
        self.table.drop_if_empty(&self.domain, &health);
        ips
    }

    pub(super) fn record_failure(&self, peer: SocketAddr) {
        self.table.ensure(&self.domain).record_failure(peer);
    }

    pub(super) fn clear_failure(&self, peer: SocketAddr) {
        let Some(health) = self.table.lookup(&self.domain) else {
            return;
        };
        health.clear_failure(peer);
        self.table.drop_if_empty(&self.domain, &health);
    }
}

pub(super) struct PeerHealthTable {
    strategy: PeerHealthCheckConfig,
    /// Outer lock only covers finding or creating a domain table.
    domains: Mutex<HashMap<DomainName, Arc<PeerHealth>>>,
}

impl PeerHealthTable {
    pub(super) fn new(strategy: PeerHealthCheckConfig) -> Arc<Self> {
        Arc::new(PeerHealthTable {
            strategy,
            domains: Mutex::new(HashMap::new()),
        })
    }

    /// Keep this table when the escaper still binds the same addresses and uses the same strategy.
    pub(super) fn on_reload(
        self: &Arc<Self>,
        same_bind: bool,
        strategy: Option<PeerHealthCheckConfig>,
    ) -> Option<Arc<Self>> {
        let strategy = strategy?;
        if same_bind && self.strategy == strategy {
            Some(Arc::clone(self))
        } else {
            Some(Self::new(strategy))
        }
    }

    /// Bind later failure records to `domain` without inserting an empty table.
    pub(super) fn scope(self: &Arc<Self>, domain: &DomainName) -> ScopedPeerHealth {
        ScopedPeerHealth {
            table: Arc::clone(self),
            domain: domain.clone(),
        }
    }

    fn lookup(&self, domain: &DomainName) -> Option<Arc<PeerHealth>> {
        self.domains.lock().unwrap().get(domain).cloned()
    }

    fn ensure(&self, domain: &DomainName) -> Arc<PeerHealth> {
        let mut domains = self.domains.lock().unwrap();
        domains
            .entry(domain.clone())
            .or_insert_with(|| PeerHealth::new(self.strategy))
            .clone()
    }

    /// Drop `domain` when `health` is still the mapped table and has no records.
    fn drop_if_empty(&self, domain: &DomainName, health: &Arc<PeerHealth>) {
        let mut domains = self.domains.lock().unwrap();
        let Some(current) = domains.get(domain) else {
            return;
        };
        if !Arc::ptr_eq(current, health) || !health.is_empty() {
            return;
        }
        domains.remove(domain);
    }
}

fn partition_unhealthy_first(ips: &mut Vec<IpAddr>, unhealthy: &[bool]) {
    let mut bad = Vec::new();
    let mut good = Vec::new();
    for (ip, is_bad) in std::mem::take(ips).into_iter().zip(unhealthy.iter()) {
        if *is_bad {
            bad.push(ip);
        } else {
            good.push(ip);
        }
    }
    ips.extend(bad);
    ips.extend(good);
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::str::FromStr;

    use super::*;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(10, 0, 0, last))
    }

    fn domain(name: &str) -> DomainName {
        DomainName::from_str(name).unwrap()
    }

    fn upstream(table: &Arc<PeerHealthTable>, name: &str) -> ScopedPeerHealth {
        table.scope(&domain(name))
    }

    fn strategy() -> PeerHealthCheckConfig {
        PeerHealthCheckConfig {
            max_fails: 1,
            fail_timeout: Duration::from_secs(30),
        }
    }

    fn new_table() -> Arc<PeerHealthTable> {
        PeerHealthTable::new(strategy())
    }

    #[test]
    fn unhealthy_address_is_tried_after_the_others() {
        let health = new_table();
        let bad = SocketAddr::new(ip(2), 443);
        upstream(&health, "proxy.example").record_failure(bad);

        let mut ips = vec![ip(1), ip(2), ip(3)];
        upstream(&health, "proxy.example").reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(2), ip(1), ip(3)]);

        upstream(&health, "proxy.example").clear_failure(bad);
        let mut ips = vec![ip(1), ip(2), ip(3)];
        upstream(&health, "proxy.example").reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2), ip(3)]);
    }

    #[test]
    fn skip_recent_failures_drops_only_the_failed_address() {
        let health = new_table();
        let peer_health = upstream(&health, "proxy.example");
        peer_health.record_failure(SocketAddr::new(ip(2), 443));

        let left = peer_health.skip_recent_failures(443, vec![ip(1), ip(2), ip(3)]);
        assert_eq!(left, vec![ip(1), ip(3)]);

        peer_health.record_failure(SocketAddr::new(ip(1), 443));
        peer_health.record_failure(SocketAddr::new(ip(3), 443));
        let left = peer_health.skip_recent_failures(443, vec![ip(1), ip(2), ip(3)]);
        assert_eq!(left, vec![ip(1), ip(2), ip(3)]);
    }

    #[test]
    fn other_port_and_domain_stay_untouched() {
        let health = new_table();
        let scoped = upstream(&health, "origin.example");
        scoped.record_failure(SocketAddr::new(ip(2), 443));

        let mut ips = vec![ip(1), ip(2)];
        scoped.reorder(80, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2)]);

        let other = upstream(&health, "other.example");
        let mut ips = vec![ip(1), ip(2)];
        other.reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2)]);

        let mut ips = vec![ip(1), ip(2)];
        scoped.reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(2), ip(1)]);
    }

    #[test]
    fn expired_failure_is_dropped() {
        let health = new_table();
        let peer = SocketAddr::new(ip(1), 443);
        upstream(&health, "proxy.example").record_failure(peer);
        {
            let view = health.lookup(&domain("proxy.example")).unwrap();
            let mut inner = view.inner.lock().unwrap();
            inner.get_mut(&peer).unwrap().until = Instant::now() - Duration::from_secs(1);
        }
        let mut ips = vec![ip(1), ip(2)];
        upstream(&health, "proxy.example").reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2)]);
        assert!(health.lookup(&domain("proxy.example")).is_none());
    }

    #[test]
    fn unchanged_bind_keeps_the_table_and_a_new_bind_drops_it() {
        let health = new_table();
        upstream(&health, "proxy.example").record_failure(SocketAddr::new(ip(1), 443));

        let kept = health.on_reload(true, Some(strategy())).unwrap();
        assert!(Arc::ptr_eq(&health, &kept));
        let mut ips = vec![ip(2), ip(1)];
        upstream(&kept, "proxy.example").reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2)]);

        let fresh = health.on_reload(false, Some(strategy())).unwrap();
        assert!(!Arc::ptr_eq(&health, &fresh));
        let mut ips = vec![ip(2), ip(1)];
        upstream(&fresh, "proxy.example").reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(2), ip(1)]);

        assert!(health.on_reload(true, None).is_none());
    }

    #[test]
    fn peer_stays_available_until_max_fails() {
        let health = PeerHealthTable::new(PeerHealthCheckConfig {
            max_fails: 3,
            fail_timeout: Duration::from_secs(30),
        });
        let peer_health = upstream(&health, "proxy.example");
        let peer = SocketAddr::new(ip(2), 443);
        peer_health.record_failure(peer);
        peer_health.record_failure(peer);

        let mut ips = vec![ip(1), ip(2)];
        peer_health.reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2)]);

        peer_health.record_failure(peer);
        let mut ips = vec![ip(1), ip(2)];
        peer_health.reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(2), ip(1)]);
    }

    #[test]
    fn domains_without_failures_are_not_stored() {
        let health = new_table();
        let scoped = upstream(&health, "origin.example");
        let mut ips = vec![ip(1), ip(2)];
        scoped.reorder(443, &mut ips);
        assert_eq!(ips, vec![ip(1), ip(2)]);
        scoped.skip_recent_failures(443, vec![ip(1), ip(2)]);
        scoped.clear_failure(SocketAddr::new(ip(1), 443));
        assert!(health.domains.lock().unwrap().is_empty());
    }

    #[test]
    fn domain_is_removed_when_its_last_failure_clears_or_expires() {
        let health = new_table();
        let name = domain("origin.example");
        let scoped = health.scope(&name);
        let peer = SocketAddr::new(ip(1), 443);
        scoped.record_failure(peer);
        assert!(health.lookup(&name).is_some());

        scoped.clear_failure(peer);
        assert!(health.lookup(&name).is_none());

        scoped.record_failure(peer);
        {
            let view = health.lookup(&name).unwrap();
            view.inner.lock().unwrap().get_mut(&peer).unwrap().until =
                Instant::now() - Duration::from_secs(1);
        }
        let mut ips = vec![ip(1)];
        scoped.reorder(80, &mut ips);
        assert!(health.lookup(&name).is_none());
    }
}
