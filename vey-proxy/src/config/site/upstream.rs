/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::BTreeSet;
use std::net::SocketAddr;

use anyhow::anyhow;
use yaml_rust::Yaml;

use vey_types::collection::{SelectivePickPolicy, WeightedValue};
use vey_types::net::UpstreamAddr;

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
                let peers = vey_yaml::value::as_list(value, vey_yaml::value::as_weighted_sockaddr)?;
                if peers.is_empty() {
                    return Err(anyhow!("upstream peer address list is empty"));
                }
                let mut seen = BTreeSet::new();
                for item in &peers {
                    if !seen.insert(*item.inner()) {
                        return Err(anyhow!("duplicate upstream peer address {}", item.inner()));
                    }
                }
                Ok(SiteUpstreamConfig {
                    single: None,
                    peers,
                    pick_policy: SelectivePickPolicy::RoundRobin,
                })
            }
            _ => Err(anyhow!(
                "upstream must be an address string or a list of weighted socket address"
            )),
        }
    }
}
