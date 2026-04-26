/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::collections::BTreeSet;
use std::net::IpAddr;
use std::time::Duration;

use anyhow::anyhow;
use ip_network::IpNetwork;

use vey_histogram::HistogramMetricsConfig;
use vey_types::metrics::NodeName;
use vey_types::net::{DomainName, Host, OpensslClientConfigBuilder};
use vey_types::resolve::ResolveStrategy;

mod json;
mod yaml;

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct UserSiteConfig {
    pub(crate) id: NodeName,
    pub(crate) exact_match_domain: BTreeSet<DomainName>,
    pub(crate) exact_match_ipaddr: BTreeSet<IpAddr>,
    pub(crate) subnet_match_ipaddr: BTreeSet<IpNetwork>,
    pub(crate) child_match_domain: BTreeSet<DomainName>,
    pub(crate) emit_stats: bool,
    pub(crate) resolve_strategy: Option<ResolveStrategy>,
    pub(crate) duration_stats: HistogramMetricsConfig,
    pub(crate) tls_client: Option<OpensslClientConfigBuilder>,
    pub(crate) http_rsp_hdr_recv_timeout: Option<Duration>,
}

impl UserSiteConfig {
    fn check(&self) -> anyhow::Result<()> {
        if self.id.is_empty() {
            return Err(anyhow!("site id is not set"));
        }
        Ok(())
    }

    fn add_exact_host(&mut self, host: Host) {
        match host {
            Host::Domain(domain) => self.exact_match_domain.insert(domain),
            Host::Ip(ip) => self.exact_match_ipaddr.insert(ip),
        };
    }
}
