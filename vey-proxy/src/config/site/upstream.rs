/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::net::SocketAddr;

use anyhow::anyhow;
use yaml_rust::Yaml;

use vey_types::collection::{SelectivePickPolicy, WeightedValue};
use vey_types::net::{Host, UpstreamAddr};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SiteUpstreamConfig {
    single: Option<UpstreamAddr>,
    peers: Vec<WeightedValue<SocketAddr>>,
    pick_policy: SelectivePickPolicy,
}

impl Default for SiteUpstreamConfig {
    fn default() -> Self {
        SiteUpstreamConfig {
            single: None,
            peers: Vec::new(),
            pick_policy: SelectivePickPolicy::RoundRobin,
        }
    }
}

impl SiteUpstreamConfig {
    pub(crate) fn is_empty(&self) -> bool {
        self.single.is_none() && self.peers.is_empty()
    }

    pub(crate) fn single(&self) -> Option<&UpstreamAddr> {
        self.single.as_ref()
    }

    pub(crate) fn peers(&self) -> &[WeightedValue<SocketAddr>] {
        &self.peers
    }

    pub(crate) fn pick_policy(&self) -> SelectivePickPolicy {
        self.pick_policy
    }

    pub(crate) fn set_pick_policy(&mut self, policy: SelectivePickPolicy) {
        self.pick_policy = policy;
    }

    pub(crate) fn set_targets(&mut self, parsed: SiteUpstreamConfig) {
        self.single = parsed.single;
        self.peers = parsed.peers;
    }

    pub(crate) fn parse(value: &Yaml) -> anyhow::Result<Self> {
        match value {
            Yaml::String(_) => {
                let addr = vey_yaml::value::as_upstream_addr(value, 80)?;
                Ok(SiteUpstreamConfig {
                    single: Some(addr),
                    peers: Vec::new(),
                    pick_policy: SelectivePickPolicy::RoundRobin,
                })
            }
            Yaml::Array(_) => {
                let weighted = vey_yaml::value::as_list(value, |v| {
                    vey_yaml::value::as_weighted_upstream_addr(v, 0)
                })?;
                if weighted.is_empty() {
                    return Err(anyhow!("upstream list is empty"));
                }
                let mut peers = Vec::with_capacity(weighted.len());
                let mut seen = BTreeSet::new();
                for item in weighted {
                    let addr = item.inner();
                    let Host::Ip(ip) = addr.host() else {
                        return Err(anyhow!(
                            "upstream list entries must be ip:port, got {}",
                            addr.host()
                        ));
                    };
                    if addr.port() == 0 {
                        return Err(anyhow!("upstream list entries must include a port"));
                    }
                    let socket = SocketAddr::new(*ip, addr.port());
                    if !seen.insert(socket) {
                        return Err(anyhow!("duplicate upstream address {socket}"));
                    }
                    let weight = item.weight();
                    if !weight.is_finite() || weight < 0.0 {
                        return Err(anyhow!("upstream weight for {socket} must be >= 0"));
                    }
                    peers.push(WeightedValue::with_weight(socket, weight));
                }
                Ok(SiteUpstreamConfig {
                    single: None,
                    peers,
                    pick_policy: SelectivePickPolicy::RoundRobin,
                })
            }
            _ => Err(anyhow!(
                "upstream must be an address string or a list of ip:port"
            )),
        }
    }
}
