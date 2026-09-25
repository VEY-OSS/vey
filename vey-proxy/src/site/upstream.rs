/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use anyhow::anyhow;
use arc_swap::ArcSwapOption;

use vey_types::collection::{
    SelectivePickPolicy, SelectiveVec, SelectiveVecBuilder, WeightedValue,
};
use vey_types::net::UpstreamAddr;

use crate::config::site::SiteUpstreamConfig;

pub(crate) struct SiteUpstream {
    config: SiteUpstreamConfig,
    /// Runtime weight for each configured peer. Missing entries use the config weight.
    runtime_weight: Mutex<HashMap<SocketAddr, f64>>,
    pick: ArcSwapOption<SelectiveVec<WeightedValue<SocketAddr>>>,
}

pub(crate) struct UpstreamPeerStatus {
    pub(crate) addr: SocketAddr,
    pub(crate) config_weight: f64,
    pub(crate) weight: f64,
}

impl SiteUpstream {
    pub(super) fn from_config(config: &SiteUpstreamConfig) -> Self {
        let upstream = SiteUpstream {
            config: config.clone(),
            runtime_weight: Mutex::new(HashMap::new()),
            pick: ArcSwapOption::empty(),
        };
        upstream.rebuild();
        upstream
    }

    pub(super) fn new_for_reload(old: &SiteUpstream, config: &SiteUpstreamConfig) -> Self {
        let upstream = SiteUpstream::from_config(config);
        if upstream.config.single().is_some() || upstream.config.peers().is_empty() {
            return upstream;
        }
        let old_weights = old.runtime_weight.lock().unwrap();
        if old_weights.is_empty() {
            return upstream;
        }
        {
            let mut weights = upstream.runtime_weight.lock().unwrap();
            for peer in upstream.config.peers() {
                let addr = *peer.inner();
                if let Some(weight) = old_weights.get(&addr) {
                    weights.insert(addr, *weight);
                }
            }
        }
        upstream.rebuild();
        upstream
    }

    pub(super) fn select(&self, client_ip: IpAddr) -> anyhow::Result<UpstreamAddr> {
        if let Some(addr) = self.config.single() {
            return Ok(addr.clone());
        }
        let Some(nodes) = self.pick.load_full() else {
            return Err(anyhow!("no upstream address with a positive weight"));
        };
        let picked = match self.config.pick_policy() {
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
        if self.config.peers().is_empty() {
            return Err(anyhow!("site upstream has no peers defined"));
        }
        let weights = self.runtime_weight.lock().unwrap();
        Ok(self
            .config
            .peers()
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

    pub(super) fn set_weight(&self, addr: SocketAddr, weight: f64) -> anyhow::Result<()> {
        if self.config.peers().is_empty() {
            return Err(anyhow!("site upstream has no peers defined"));
        }
        if !weight.is_finite() || weight < 0.0 {
            return Err(anyhow!("weight must be a finite number >= 0"));
        }
        if !self.config.peers().iter().any(|peer| *peer.inner() == addr) {
            return Err(anyhow!("upstream address {addr} is not configured"));
        }
        self.runtime_weight.lock().unwrap().insert(addr, weight);
        self.rebuild();
        Ok(())
    }

    fn rebuild(&self) {
        if self.config.peers().is_empty() {
            self.pick.store(None);
            return;
        }
        let weights = self.runtime_weight.lock().unwrap();
        let mut builder = SelectiveVecBuilder::new();
        for peer in self.config.peers() {
            let addr = *peer.inner();
            let weight = weights.get(&addr).copied().unwrap_or(peer.weight());
            if weight.is_finite() && weight > 0.0 {
                builder.insert(WeightedValue::with_weight(addr, weight));
            }
        }
        drop(weights);
        self.pick.store(builder.build().map(std::sync::Arc::new));
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
        let upstream = SiteUpstream::from_config(&config);
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
        let upstream = SiteUpstream::from_config(&config);
        let addr = SocketAddr::from_str("10.0.0.1:8080").unwrap();
        upstream.set_weight(addr, 0.0).unwrap();
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(upstream.select(ip).unwrap().to_string(), "10.0.0.2:8080");

        let reloaded = SiteUpstream::new_for_reload(&upstream, &config);
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
        let upstream = SiteUpstream::from_config(&config);
        upstream
            .set_weight(SocketAddr::from_str("10.0.0.1:8080").unwrap(), 0.0)
            .unwrap();
        assert!(upstream.select(IpAddr::V4(Ipv4Addr::LOCALHOST)).is_err());
    }
}
