/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use vey_types::net::DomainName;

use crate::config::escaper::PeerHealthCheckConfig;

const DIRECT_SWEEP_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct Failure {
    count: u32,
    until: Instant,
}

/// Failures for one domain. Each domain has its own lock.
pub(crate) struct PeerHealth {
    strategy: PeerHealthCheckConfig,
    inner: Mutex<HashMap<SocketAddr, Failure>>,
}

impl PeerHealth {
    pub(crate) fn new(strategy: PeerHealthCheckConfig) -> Arc<Self> {
        Arc::new(PeerHealth {
            strategy,
            inner: Mutex::new(HashMap::new()),
        })
    }

    /// New table with `strategy`, keeping the current failure records.
    pub(crate) fn rebuild(self: &Arc<Self>, strategy: PeerHealthCheckConfig) -> Arc<Self> {
        if self.strategy == strategy {
            Arc::clone(self)
        } else {
            Arc::new(PeerHealth {
                strategy,
                inner: Mutex::new(self.inner.lock().unwrap().clone()),
            })
        }
    }

    /// Remove a failure record.
    /// Returns whether the peer left the unavailable set, so the pick table should be rebuilt.
    pub(crate) fn clear_failure(&self, peer: SocketAddr) -> bool {
        self.clear_failure_using(peer, self.strategy)
    }

    fn clear_failure_using(&self, peer: SocketAddr, strategy: PeerHealthCheckConfig) -> bool {
        let max_fails = strategy.max_fails;
        let now = Instant::now();
        match self.inner.lock().unwrap().remove(&peer) {
            Some(failure) => failure.until > now && failure.count >= max_fails,
            None => false,
        }
    }

    /// Record one failed connect.
    /// Returns whether the peer entered or left the unavailable set.
    pub(crate) fn record_failure(&self, peer: SocketAddr) -> bool {
        self.record_failure_using(peer, self.strategy)
    }

    fn record_failure_using(&self, peer: SocketAddr, strategy: PeerHealthCheckConfig) -> bool {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        let entry = inner.entry(peer).or_insert(Failure {
            count: 0,
            until: now,
        });
        let was_unavailable = entry.until > now && entry.count >= strategy.max_fails;
        if entry.until <= now {
            entry.count = 1;
            entry.until = now + strategy.fail_timeout;
        } else {
            entry.count = entry.count.saturating_add(1);
            if entry.count >= strategy.max_fails {
                entry.until = now + strategy.fail_timeout;
            }
        }
        was_unavailable != (entry.count >= strategy.max_fails)
    }

    /// Drop addresses that recently failed. If every address failed, keep the full list.
    fn skip_recent_failures_using(
        &self,
        port: u16,
        ips: Vec<IpAddr>,
        strategy: PeerHealthCheckConfig,
    ) -> Vec<IpAddr> {
        if ips.len() < 2 {
            return ips;
        }
        let max_fails = strategy.max_fails;
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        let healthy: Vec<IpAddr> = ips
            .iter()
            .copied()
            .filter(|ip| !self.is_unhealthy(&mut inner, SocketAddr::new(*ip, port), now, max_fails))
            .collect();
        if healthy.is_empty() { ips } else { healthy }
    }

    pub(crate) fn unavailable(&self, peer: SocketAddr) -> bool {
        let max_fails = self.strategy().max_fails;
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        self.is_unhealthy(&mut inner, peer, now, max_fails)
    }

    pub(crate) fn strategy(&self) -> PeerHealthCheckConfig {
        self.strategy
    }

    /// Drop expired records. Returns whether an unavailable peer was removed.
    pub(crate) fn sweep_expired(&self) -> bool {
        self.sweep_expired_using(self.strategy)
    }

    fn sweep_expired_using(&self, strategy: PeerHealthCheckConfig) -> bool {
        let max_fails = strategy.max_fails;
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        let mut removed_unavailable = false;
        inner.retain(|_, failure| {
            if failure.until > now {
                true
            } else {
                if failure.count >= max_fails {
                    removed_unavailable = true;
                }
                false
            }
        });
        removed_unavailable
    }

    pub(crate) fn failure_status(&self, peer: SocketAddr) -> (u32, bool, Option<Duration>) {
        let max_fails = self.strategy().max_fails;
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap();
        match inner
            .get(&peer)
            .map(|failure| (failure.count, failure.until))
        {
            Some((count, until)) if until > now => {
                let unavailable = count >= max_fails;
                let recover_in = unavailable.then(|| until.saturating_duration_since(now));
                (count, unavailable, recover_in)
            }
            Some(_) => {
                inner.remove(&peer);
                (0, false, None)
            }
            None => (0, false, None),
        }
    }

    fn is_unhealthy(
        &self,
        inner: &mut HashMap<SocketAddr, Failure>,
        peer: SocketAddr,
        now: Instant,
        max_fails: u32,
    ) -> bool {
        match inner
            .get(&peer)
            .map(|failure| (failure.count, failure.until))
        {
            Some((count, until)) if until > now && count >= max_fails => true,
            Some((_, until)) if until <= now => {
                inner.remove(&peer);
                false
            }
            _ => false,
        }
    }

    fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    fn drop_expired(&self, now: Instant) {
        self.inner
            .lock()
            .unwrap()
            .retain(|_, failure| failure.until > now);
    }
}

/// One domain in [`PeerHealthTable`]. Looking a domain up does not insert it.
/// `strategy` comes from the site that enabled the check for this task.
pub(super) struct ScopedPeerHealth {
    table: Arc<PeerHealthTable>,
    domain: DomainName,
    strategy: PeerHealthCheckConfig,
}

impl ScopedPeerHealth {
    /// Whether `peer` should be left out of a limited candidate list.
    /// Expired records are dropped as part of the check.
    pub(super) fn should_defer(&self, peer: SocketAddr) -> bool {
        let Some(health) = self.table.lookup(&self.domain) else {
            return false;
        };
        health.sweep_expired_using(self.strategy);
        let max_fails = self.strategy.max_fails;
        let now = Instant::now();
        let unavailable = {
            let mut inner = health.inner.lock().unwrap();
            health.is_unhealthy(&mut inner, peer, now, max_fails)
        };
        self.table.drop_if_empty(&self.domain, &health);
        unavailable
    }

    pub(super) fn skip_recent_failures(&self, port: u16, ips: Vec<IpAddr>) -> Vec<IpAddr> {
        let Some(health) = self.table.lookup(&self.domain) else {
            return ips;
        };
        health.sweep_expired_using(self.strategy);
        let ips = health.skip_recent_failures_using(port, ips, self.strategy);
        self.table.drop_if_empty(&self.domain, &health);
        ips
    }

    pub(super) fn record_failure(&self, peer: SocketAddr) -> bool {
        self.table
            .ensure(&self.domain)
            .record_failure_using(peer, self.strategy)
    }

    pub(super) fn clear_failure(&self, peer: SocketAddr) -> bool {
        let Some(health) = self.table.lookup(&self.domain) else {
            return false;
        };
        let changed = health.clear_failure_using(peer, self.strategy);
        self.table.drop_if_empty(&self.domain, &health);
        changed
    }
}

struct DomainSlot {
    health: Arc<PeerHealth>,
    touched: Instant,
}

/// Failure records for addresses reached through one escaper.
/// Limits come from the site on each use, because sites sharing an escaper
/// still leave through the same path.
pub(super) struct PeerHealthTable {
    /// Outer lock only covers finding or creating a domain table.
    domains: Mutex<HashMap<DomainName, DomainSlot>>,
    /// `usize::MAX` does not evict. Direct escapers set this from config.
    max_domains: AtomicUsize,
}

impl PeerHealthTable {
    pub(super) fn new() -> Arc<Self> {
        Self::with_limit(usize::MAX)
    }

    /// Bounded table for `direct_fixed` and `direct_float`.
    /// Expired domains are dropped on a timer, including names nobody asks
    /// for again. A full table drops the least recently used domain.
    pub(super) fn new_direct(max_domains: usize) -> Arc<Self> {
        let table = Self::with_limit(max_domains);
        table.spawn_sweeper();
        table
    }

    fn with_limit(max_domains: usize) -> Arc<Self> {
        Arc::new(PeerHealthTable {
            domains: Mutex::new(HashMap::new()),
            max_domains: AtomicUsize::new(max_domains),
        })
    }

    /// Apply a new cap. A smaller cap drops the least recently used domains now.
    pub(super) fn set_max_domains(&self, max_domains: usize) {
        self.max_domains.store(max_domains, Ordering::Relaxed);
        if max_domains == usize::MAX {
            return;
        }
        let mut domains = self.domains.lock().unwrap();
        retain_live(&mut domains, Instant::now());
        while domains.len() > max_domains && pop_oldest(&mut domains) {}
    }

    fn spawn_sweeper(self: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let weak = Arc::downgrade(self);
        drop(handle.spawn(async move {
            let mut interval = tokio::time::interval(DIRECT_SWEEP_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                interval.tick().await;
                let Some(table) = weak.upgrade() else {
                    break;
                };
                table.sweep_expired_domains();
            }
        }));
    }

    fn sweep_expired_domains(&self) {
        let mut domains = self.domains.lock().unwrap();
        retain_live(&mut domains, Instant::now());
    }

    /// Keep recorded failures when the bind addresses stay the same.
    /// A new bind starts empty, because the source path changed.
    pub(super) fn on_reload(self: &Arc<Self>, same_bind: bool) -> Arc<Self> {
        if same_bind {
            Arc::clone(self)
        } else if self.max_domains.load(Ordering::Relaxed) == usize::MAX {
            Self::new()
        } else {
            let table = Self::with_limit(self.max_domains.load(Ordering::Relaxed));
            table.spawn_sweeper();
            table
        }
    }

    /// Bind later failure records to `domain` without inserting an empty table.
    pub(super) fn scope(
        self: &Arc<Self>,
        domain: &DomainName,
        strategy: PeerHealthCheckConfig,
    ) -> ScopedPeerHealth {
        ScopedPeerHealth {
            table: Arc::clone(self),
            domain: domain.clone(),
            strategy,
        }
    }

    fn lookup(&self, domain: &DomainName) -> Option<Arc<PeerHealth>> {
        let mut domains = self.domains.lock().unwrap();
        let slot = domains.get_mut(domain)?;
        slot.touched = Instant::now();
        Some(Arc::clone(&slot.health))
    }

    fn ensure(&self, domain: &DomainName) -> Arc<PeerHealth> {
        let mut domains = self.domains.lock().unwrap();
        if let Some(slot) = domains.get_mut(domain) {
            slot.touched = Instant::now();
            return Arc::clone(&slot.health);
        }
        let max_domains = self.max_domains.load(Ordering::Relaxed);
        if max_domains != usize::MAX {
            retain_live(&mut domains, Instant::now());
            while domains.len() >= max_domains && pop_oldest(&mut domains) {}
        }
        let health = PeerHealth::new(PeerHealthCheckConfig::default());
        domains.insert(
            domain.clone(),
            DomainSlot {
                health: Arc::clone(&health),
                touched: Instant::now(),
            },
        );
        health
    }

    /// Drop `domain` when `health` is still the mapped table and has no records.
    fn drop_if_empty(&self, domain: &DomainName, health: &Arc<PeerHealth>) {
        let mut domains = self.domains.lock().unwrap();
        let Some(current) = domains.get(domain) else {
            return;
        };
        if !Arc::ptr_eq(&current.health, health) || !health.is_empty() {
            return;
        }
        domains.remove(domain);
    }
}

fn retain_live(domains: &mut HashMap<DomainName, DomainSlot>, now: Instant) {
    domains.retain(|_, slot| {
        slot.health.drop_expired(now);
        !slot.health.is_empty()
    });
}

fn pop_oldest(domains: &mut HashMap<DomainName, DomainSlot>) -> bool {
    let oldest = domains
        .iter()
        .min_by_key(|(_, slot)| slot.touched)
        .map(|(name, _)| name.clone());
    match oldest {
        Some(oldest) => {
            domains.remove(&oldest);
            true
        }
        None => false,
    }
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
        table.scope(&domain(name), strategy())
    }

    fn strategy() -> PeerHealthCheckConfig {
        PeerHealthCheckConfig {
            max_fails: 1,
            fail_timeout: Duration::from_secs(30),
        }
    }

    fn new_table() -> Arc<PeerHealthTable> {
        PeerHealthTable::new()
    }

    #[test]
    fn recorded_failure_is_deferred_until_cleared() {
        let health = new_table();
        let bad = SocketAddr::new(ip(2), 443);
        let scoped = upstream(&health, "proxy.example");
        scoped.record_failure(bad);

        assert!(scoped.should_defer(bad));
        assert!(!scoped.should_defer(SocketAddr::new(ip(1), 443)));
        assert!(!scoped.should_defer(SocketAddr::new(ip(3), 443)));

        scoped.clear_failure(bad);
        assert!(!scoped.should_defer(bad));
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

        assert!(!scoped.should_defer(SocketAddr::new(ip(2), 80)));
        let other = upstream(&health, "other.example");
        assert!(!other.should_defer(SocketAddr::new(ip(2), 443)));
        assert!(scoped.should_defer(SocketAddr::new(ip(2), 443)));
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
        assert!(!upstream(&health, "proxy.example").should_defer(peer));
        assert!(health.lookup(&domain("proxy.example")).is_none());
    }

    #[test]
    fn unchanged_bind_keeps_the_table_and_a_new_bind_drops_it() {
        let health = new_table();
        upstream(&health, "proxy.example").record_failure(SocketAddr::new(ip(1), 443));

        let kept = health.on_reload(true);
        assert!(Arc::ptr_eq(&health, &kept));
        let peer = SocketAddr::new(ip(1), 443);
        assert!(upstream(&kept, "proxy.example").should_defer(peer));

        let fresh = health.on_reload(false);
        assert!(!Arc::ptr_eq(&health, &fresh));
        assert!(!upstream(&fresh, "proxy.example").should_defer(peer));

        let loose = health.scope(
            &domain("proxy.example"),
            PeerHealthCheckConfig {
                max_fails: 3,
                fail_timeout: Duration::from_secs(30),
            },
        );
        assert!(!loose.should_defer(peer));
    }

    #[test]
    fn peer_stays_available_until_max_fails() {
        let health = new_table();
        let peer_health = health.scope(
            &domain("proxy.example"),
            PeerHealthCheckConfig {
                max_fails: 3,
                fail_timeout: Duration::from_secs(30),
            },
        );
        let peer = SocketAddr::new(ip(2), 443);
        assert!(!peer_health.record_failure(peer));
        assert!(!peer_health.record_failure(peer));
        assert!(!peer_health.should_defer(peer));

        assert!(peer_health.record_failure(peer));
        assert!(peer_health.should_defer(peer));

        assert!(!peer_health.record_failure(peer));
        assert!(peer_health.clear_failure(peer));
        assert!(!peer_health.clear_failure(peer));
        assert!(!peer_health.should_defer(peer));
    }

    #[test]
    fn domains_without_failures_are_not_stored() {
        let health = new_table();
        let scoped = upstream(&health, "origin.example");
        assert!(!scoped.should_defer(SocketAddr::new(ip(1), 443)));
        scoped.skip_recent_failures(443, vec![ip(1), ip(2)]);
        assert!(!scoped.clear_failure(SocketAddr::new(ip(1), 443)));
        assert!(health.domains.lock().unwrap().is_empty());
    }

    #[test]
    fn domain_is_removed_when_its_last_failure_clears_or_expires() {
        let health = new_table();
        let name = domain("origin.example");
        let scoped = health.scope(&name, strategy());
        let peer = SocketAddr::new(ip(1), 443);
        scoped.record_failure(peer);
        assert!(health.lookup(&name).is_some());

        assert!(scoped.clear_failure(peer));
        assert!(health.lookup(&name).is_none());

        scoped.record_failure(peer);
        {
            let view = health.lookup(&name).unwrap();
            view.inner.lock().unwrap().get_mut(&peer).unwrap().until =
                Instant::now() - Duration::from_secs(1);
        }
        assert!(!scoped.should_defer(SocketAddr::new(ip(1), 80)));
        assert!(health.lookup(&name).is_none());
    }

    #[test]
    fn limited_pick_keeps_healthy_addresses_past_the_failed_prefix() {
        use vey_types::resolve::{PickStrategy, ResolveStrategy};

        let health = new_table();
        let scoped = upstream(&health, "proxy.example");
        for last in 1..=3 {
            scoped.record_failure(SocketAddr::new(ip(last), 443));
        }
        let strategy = ResolveStrategy {
            pick: PickStrategy::Serial,
            ..ResolveStrategy::default()
        };
        let defer = |addr: &IpAddr| scoped.should_defer(SocketAddr::new(*addr, 443));
        let all = vec![ip(1), ip(2), ip(3), ip(4), ip(5)];
        assert_eq!(
            strategy.pick_many_deferring(all.clone(), 2, defer),
            vec![ip(4), ip(5)]
        );
        assert_eq!(
            strategy.pick_many_deferring(all, 4, defer),
            vec![ip(1), ip(2), ip(4), ip(5)]
        );
        assert!(!scoped.should_defer(SocketAddr::new(ip(4), 443)));
        assert!(!scoped.should_defer(SocketAddr::new(ip(1), 80)));
    }

    fn expire(table: &PeerHealthTable, name: &str, peer: SocketAddr) {
        let view = table.lookup(&domain(name)).unwrap();
        view.inner.lock().unwrap().get_mut(&peer).unwrap().until =
            Instant::now() - Duration::from_secs(1);
    }

    #[test]
    fn sweep_drops_expired_domains_nobody_queries_again() {
        let health = PeerHealthTable::with_limit(8);
        let peer = SocketAddr::new(ip(1), 443);
        upstream(&health, "a.example").record_failure(peer);
        upstream(&health, "b.example").record_failure(peer);
        expire(&health, "a.example", peer);

        health.sweep_expired_domains();

        assert!(health.lookup(&domain("a.example")).is_none());
        assert!(health.lookup(&domain("b.example")).is_some());
    }

    #[test]
    fn full_direct_table_drops_the_least_recently_used_domain() {
        let health = PeerHealthTable::with_limit(2);
        let peer = SocketAddr::new(ip(1), 443);
        upstream(&health, "a.example").record_failure(peer);
        upstream(&health, "b.example").record_failure(peer);
        let now = Instant::now();
        {
            let mut domains = health.domains.lock().unwrap();
            domains.get_mut(&domain("a.example")).unwrap().touched = now;
            domains.get_mut(&domain("b.example")).unwrap().touched = now + Duration::from_secs(1);
        }

        upstream(&health, "c.example").record_failure(peer);

        assert!(health.lookup(&domain("a.example")).is_none());
        assert!(health.lookup(&domain("b.example")).is_some());
        assert!(health.lookup(&domain("c.example")).is_some());
    }

    #[test]
    fn expired_domain_does_not_take_a_slot_from_the_cap() {
        let health = PeerHealthTable::with_limit(1);
        let peer = SocketAddr::new(ip(1), 443);
        upstream(&health, "a.example").record_failure(peer);
        expire(&health, "a.example", peer);

        upstream(&health, "b.example").record_failure(peer);

        assert!(health.lookup(&domain("a.example")).is_none());
        assert!(health.lookup(&domain("b.example")).is_some());
        assert_eq!(health.domains.lock().unwrap().len(), 1);
    }

    #[test]
    fn reload_of_a_bounded_table_keeps_the_limit() {
        let health = PeerHealthTable::with_limit(2);
        let fresh = health.on_reload(false);
        assert_eq!(fresh.max_domains.load(Ordering::Relaxed), 2);
        assert!(!Arc::ptr_eq(&health, &fresh));

        let kept = health.on_reload(true);
        assert!(Arc::ptr_eq(&health, &kept));
    }

    #[test]
    fn lowering_the_cap_drops_the_least_recently_used_domain() {
        let health = PeerHealthTable::with_limit(3);
        let peer = SocketAddr::new(ip(1), 443);
        upstream(&health, "a.example").record_failure(peer);
        upstream(&health, "b.example").record_failure(peer);
        upstream(&health, "c.example").record_failure(peer);
        let now = Instant::now();
        {
            let mut domains = health.domains.lock().unwrap();
            domains.get_mut(&domain("a.example")).unwrap().touched = now;
            domains.get_mut(&domain("b.example")).unwrap().touched = now + Duration::from_secs(1);
            domains.get_mut(&domain("c.example")).unwrap().touched = now + Duration::from_secs(2);
        }

        health.set_max_domains(1);

        assert!(health.lookup(&domain("a.example")).is_none());
        assert!(health.lookup(&domain("b.example")).is_none());
        assert!(health.lookup(&domain("c.example")).is_some());
        assert_eq!(health.max_domains.load(Ordering::Relaxed), 1);
    }
}
