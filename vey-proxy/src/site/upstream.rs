/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::anyhow;
use arc_swap::ArcSwapOption;

use vey_types::collection::{
    SelectivePickPolicy, SelectiveVec, SelectiveVecBuilder, WeightedValue,
};
use vey_types::net::{Host, UpstreamAddr};

use crate::config::escaper::PeerHealthCheckConfig;
use crate::config::site::SiteUpstreamConfig;
use crate::escape::PeerHealth;

pub(crate) struct SiteUpstream {
    single: Option<UpstreamAddr>,
    peers: Vec<WeightedValue<SocketAddr>>,
    policy: SelectivePickPolicy,
    /// Runtime weight for each configured peer. Missing entries use the config weight.
    runtime_weight: Mutex<HashMap<SocketAddr, f64>>,
    pick: ArcSwapOption<SelectiveVec<WeightedValue<SocketAddr>>>,
    health: Option<Arc<PeerHealth>>,
}

pub(crate) struct UpstreamPeerStatus {
    pub(crate) addr: SocketAddr,
    pub(crate) config_weight: f64,
    pub(crate) weight: f64,
}

pub(crate) struct UpstreamPeerHealthStatus {
    pub(crate) addr: SocketAddr,
    pub(crate) fails: u32,
    pub(crate) unavailable: bool,
    pub(crate) recover_in: Option<Duration>,
}

impl SiteUpstream {
    pub(super) fn from_config(
        config: &SiteUpstreamConfig,
        health: Option<PeerHealthCheckConfig>,
    ) -> Self {
        let upstream = SiteUpstream {
            single: config.single().cloned(),
            peers: config.peers().to_vec(),
            policy: config.pick_policy(),
            runtime_weight: Mutex::new(HashMap::new()),
            pick: ArcSwapOption::empty(),
            health: health.map(PeerHealth::new),
        };
        upstream.rebuild();
        upstream
    }

    pub(super) fn new_for_reload(
        old: &SiteUpstream,
        config: &SiteUpstreamConfig,
        health: Option<PeerHealthCheckConfig>,
    ) -> Self {
        let mut upstream = SiteUpstream::from_config(config, None);
        upstream.health = match (&old.health, health) {
            (Some(table), Some(strategy)) => Some(table.rebuild(strategy)),
            (_, Some(strategy)) => Some(PeerHealth::new(strategy)),
            (_, None) => None,
        };
        if upstream.single.is_none() && !upstream.peers.is_empty() {
            let old_weights = old.runtime_weight.lock().unwrap();
            if !old_weights.is_empty() {
                let mut weights = upstream.runtime_weight.lock().unwrap();
                for peer in &upstream.peers {
                    let addr = *peer.inner();
                    if let Some(weight) = old_weights.get(&addr) {
                        weights.insert(addr, *weight);
                    }
                }
            }
        }
        upstream.rebuild();
        upstream
    }

    pub(super) fn select(&self, client_ip: IpAddr) -> anyhow::Result<UpstreamAddr> {
        if let Some(addr) = &self.single {
            return Ok(addr.clone());
        }
        let Some(nodes) = self.pick.load_full() else {
            return Err(anyhow!("no upstream address with a positive weight"));
        };
        let picked = match self.policy {
            SelectivePickPolicy::Random => nodes.pick_random(),
            SelectivePickPolicy::Serial => nodes.pick_serial(),
            SelectivePickPolicy::RoundRobin => nodes.pick_round_robin(),
            SelectivePickPolicy::Ketama => nodes.pick_ketama(&client_ip),
            SelectivePickPolicy::Rendezvous => nodes.pick_rendezvous(&client_ip),
            SelectivePickPolicy::JumpHash => nodes.pick_jump(&client_ip),
        };
        Ok(UpstreamAddr::from(*picked.inner()))
    }

    pub(super) fn list_peers(&self) -> anyhow::Result<Vec<UpstreamPeerStatus>> {
        if self.single.is_some() || self.peers.is_empty() {
            return Err(anyhow!(
                "site upstream is a single address, not a weighted IP list"
            ));
        }
        let weights = self.runtime_weight.lock().unwrap();
        Ok(self
            .peers
            .iter()
            .map(|peer| {
                let addr = *peer.inner();
                UpstreamPeerStatus {
                    addr,
                    config_weight: peer.weight(),
                    weight: weights.get(&addr).copied().unwrap_or(peer.weight()),
                }
            })
            .collect())
    }

    pub(super) fn list_peer_health_status(&self) -> anyhow::Result<Vec<UpstreamPeerHealthStatus>> {
        if self.single.is_some() || self.peers.is_empty() {
            return Err(anyhow!(
                "site upstream is a single address, not a weighted IP list"
            ));
        }
        let Some(health) = &self.health else {
            return Err(anyhow!("peer health check is not enabled"));
        };
        if health.sweep_expired() {
            self.rebuild();
        }
        Ok(self
            .peers
            .iter()
            .map(|peer| {
                let addr = *peer.inner();
                let (fails, unavailable, recover_in) = health.failure_status(addr);
                UpstreamPeerHealthStatus {
                    addr,
                    fails,
                    unavailable,
                    recover_in,
                }
            })
            .collect())
    }

    pub(super) fn set_weight(&self, addr: SocketAddr, weight: f64) -> anyhow::Result<()> {
        if self.single.is_some() || self.peers.is_empty() {
            return Err(anyhow!(
                "site upstream is a single address, not a weighted IP list"
            ));
        }
        if !weight.is_finite() || weight < 0.0 {
            return Err(anyhow!("weight must be a finite number >= 0"));
        }
        if !self.peers.iter().any(|peer| *peer.inner() == addr) {
            return Err(anyhow!("upstream address {addr} is not configured"));
        }
        self.runtime_weight.lock().unwrap().insert(addr, weight);
        self.rebuild();
        Ok(())
    }

    pub(super) fn record_peer_connect_result(&self, addr: &UpstreamAddr, connected: bool) {
        let Some(health) = &self.health else {
            return;
        };
        let Some(peer) = upstream_pool_peer(addr) else {
            return;
        };
        if !self.peers.iter().any(|item| *item.inner() == peer) {
            return;
        }
        let expired = health.sweep_expired();
        let changed = if connected {
            health.clear_failure(peer)
        } else {
            health.record_failure(peer)
        };
        if expired || changed {
            self.rebuild();
        }
    }

    fn rebuild(&self) {
        if self.peers.is_empty() {
            self.pick.store(None);
            return;
        }
        let weights = self.runtime_weight.lock().unwrap();
        let mut peers = Vec::new();
        for peer in &self.peers {
            let addr = *peer.inner();
            let weight = weights.get(&addr).copied().unwrap_or(peer.weight());
            if weight.is_finite() && weight > 0.0 {
                peers.push(WeightedValue::with_weight(addr, weight));
            }
        }
        drop(weights);
        let peers = self.without_unavailable(peers);
        let mut builder = SelectiveVecBuilder::new();
        for peer in peers {
            builder.insert(peer);
        }
        self.pick.store(builder.build().map(Arc::new));
    }

    fn without_unavailable(
        &self,
        peers: Vec<WeightedValue<SocketAddr>>,
    ) -> Vec<WeightedValue<SocketAddr>> {
        let Some(health) = &self.health else {
            return peers;
        };
        if peers.len() < 2 {
            return peers;
        }
        let healthy: Vec<_> = peers
            .iter()
            .cloned()
            .filter(|peer| !health.unavailable(*peer.inner()))
            .collect();
        if healthy.is_empty() { peers } else { healthy }
    }
}

pub(crate) fn upstream_pool_peer(addr: &UpstreamAddr) -> Option<SocketAddr> {
    match addr.host() {
        Host::Ip(ip) => Some(SocketAddr::new(*ip, addr.port())),
        Host::Domain(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::str::FromStr;

    use yaml_rust::YamlLoader;

    use super::*;

    fn parse(yaml: &str) -> SiteUpstreamConfig {
        let docs = YamlLoader::load_from_str(yaml).unwrap();
        SiteUpstreamConfig::parse(&docs[0]).unwrap()
    }

    #[test]
    fn round_robin_follows_smooth_weights() {
        let config = parse(
            r#"
- 10.0.0.1:8080
- addr: 10.0.0.2:8080
  weight: 2
"#,
        );
        let upstream = SiteUpstream::from_config(&config, None);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let mut picked = Vec::new();
        for _ in 0..3 {
            picked.push(upstream.select(ip).unwrap().to_string());
        }
        assert_eq!(picked, ["10.0.0.2:8080", "10.0.0.1:8080", "10.0.0.2:8080"]);
    }

    #[test]
    fn zero_weight_drops_the_peer_and_reload_keeps_runtime_weight() {
        let config = parse(
            r#"
- 10.0.0.1:8080
- 10.0.0.2:8080
"#,
        );
        let upstream = SiteUpstream::from_config(&config, None);
        let addr = SocketAddr::from_str("10.0.0.1:8080").unwrap();
        upstream.set_weight(addr, 0.0).unwrap();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.2:8080");

        let reloaded = SiteUpstream::new_for_reload(&upstream, &config, None);
        assert_eq!(reloaded.select(ip).unwrap().to_string(), "10.0.0.2:8080");
        let peers = reloaded.list_peers().unwrap();
        let kept = peers.iter().find(|peer| peer.addr == addr).unwrap();
        assert_eq!(kept.config_weight, 1.0);
        assert_eq!(kept.weight, 0.0);
    }

    #[test]
    fn all_zero_weights_fail_selection() {
        let config = parse(
            r#"
- 10.0.0.1:8080
"#,
        );
        let upstream = SiteUpstream::from_config(&config, None);
        upstream
            .set_weight(SocketAddr::from_str("10.0.0.1:8080").unwrap(), 0.0)
            .unwrap();
        assert!(upstream.select(IpAddr::V4(Ipv4Addr::LOCALHOST)).is_err());
    }

    fn health_strategy(max_fails: u32) -> PeerHealthCheckConfig {
        PeerHealthCheckConfig {
            max_fails,
            fail_timeout: std::time::Duration::from_secs(30),
        }
    }

    fn serial_peers() -> SiteUpstreamConfig {
        let mut config = parse(
            r#"
- addr: 10.0.0.1:8080
  weight: 2
- 10.0.0.2:8080
"#,
        );
        config.set_pick_policy(SelectivePickPolicy::Serial);
        config
    }

    #[test]
    fn unhealthy_peer_is_skipped_until_every_peer_failed() {
        let config = serial_peers();
        let upstream = SiteUpstream::from_config(&config, Some(health_strategy(1)));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let bad = SocketAddr::from_str("10.0.0.1:8080").unwrap();
        let bad_addr = UpstreamAddr::from(bad);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.1:8080");

        upstream.record_peer_connect_result(&bad_addr, false);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.2:8080");

        let other = UpstreamAddr::from(SocketAddr::from_str("10.0.0.2:8080").unwrap());
        upstream.record_peer_connect_result(&other, false);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.1:8080");

        upstream.record_peer_connect_result(&bad_addr, true);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.1:8080");
    }

    #[test]
    fn peer_stays_selectable_until_max_fails() {
        let upstream = SiteUpstream::from_config(&serial_peers(), Some(health_strategy(2)));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let bad = UpstreamAddr::from(SocketAddr::from_str("10.0.0.1:8080").unwrap());
        upstream.record_peer_connect_result(&bad, false);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.1:8080");
        upstream.record_peer_connect_result(&bad, false);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.2:8080");
    }

    #[test]
    fn reload_updates_strategy_and_keeps_recorded_failures() {
        let config = serial_peers();
        let upstream = SiteUpstream::from_config(&config, Some(health_strategy(2)));
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let bad = UpstreamAddr::from(SocketAddr::from_str("10.0.0.1:8080").unwrap());
        upstream.record_peer_connect_result(&bad, false);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.1:8080");

        let tightened = SiteUpstream::new_for_reload(&upstream, &config, Some(health_strategy(1)));
        assert_eq!(tightened.select(ip).unwrap().to_string(), "10.0.0.2:8080");

        let dropped = SiteUpstream::new_for_reload(&upstream, &config, None);
        assert_eq!(dropped.select(ip).unwrap().to_string(), "10.0.0.1:8080");
    }

    #[test]
    fn list_peer_health_status_reports_recorded_failures() {
        let upstream = SiteUpstream::from_config(&serial_peers(), Some(health_strategy(1)));
        let bad = SocketAddr::from_str("10.0.0.1:8080").unwrap();
        upstream.record_peer_connect_result(&UpstreamAddr::from(bad), false);

        let peers = upstream.list_peer_health_status().unwrap();
        let failed = peers.iter().find(|peer| peer.addr == bad).unwrap();
        assert_eq!(failed.fails, 1);
        assert!(failed.unavailable);
        assert!(failed.recover_in.is_some());
        let other = peers.iter().find(|peer| peer.addr != bad).unwrap();
        assert_eq!(other.fails, 0);
        assert!(!other.unavailable);
        assert!(other.recover_in.is_none());

        let plain = SiteUpstream::from_config(&serial_peers(), None);
        assert!(plain.list_peer_health_status().is_err());
        let single = SiteUpstream::from_config(&parse("10.0.0.1:8080"), Some(health_strategy(1)));
        assert!(single.list_peer_health_status().is_err());
    }
}
