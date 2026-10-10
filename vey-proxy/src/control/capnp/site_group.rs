/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::anyhow;

use vey_types::metrics::NodeName;

use vey_proxy_proto::site_group_capnp::site_group_control;

use super::set_operation_result;
use crate::site::{Site, SiteGroup};

pub(super) struct SiteGroupControlImpl {
    name: NodeName,
}

impl SiteGroupControlImpl {
    pub(super) fn new_client(name: &str) -> anyhow::Result<site_group_control::Client> {
        let name =
            NodeName::from_str(name).map_err(|e| anyhow!("invalid site group name {name}: {e}"))?;
        if crate::site::get(&name).is_none() {
            return Err(anyhow!("no site group {name} found"));
        }
        Ok(capnp_rpc::new_client(SiteGroupControlImpl { name }))
    }

    fn group(&self) -> anyhow::Result<Arc<SiteGroup>> {
        crate::site::get(&self.name).ok_or_else(|| anyhow!("no site group {} found", self.name))
    }

    fn site(&self, site_id: &str) -> anyhow::Result<Arc<Site>> {
        let id =
            NodeName::from_str(site_id).map_err(|e| anyhow!("invalid site id {site_id}: {e}"))?;
        self.group()?
            .site(&id)
            .ok_or_else(|| anyhow!("no site {id} in site group {}", self.name))
    }
}

impl site_group_control::Server for SiteGroupControlImpl {
    async fn list_upstream(
        self: Rc<Self>,
        params: site_group_control::ListUpstreamParams,
        mut results: site_group_control::ListUpstreamResults,
    ) -> capnp::Result<()> {
        let site_id = params.get()?.get_site_id()?.to_str()?;
        match self
            .site(site_id)
            .and_then(|site| site.list_upstream_peers())
        {
            Ok(peers) => {
                let mut builder = results.get().init_result().init_peers(peers.len() as u32);
                for (i, peer) in peers.iter().enumerate() {
                    let mut item = builder.reborrow().get(i as u32);
                    item.set_addr(peer.addr.to_string().as_str());
                    item.set_config_weight(peer.config_weight);
                    item.set_weight(peer.weight);
                }
            }
            Err(e) => {
                let mut ev = results.get().init_result().init_err();
                ev.set_code(-1);
                ev.set_reason(format!("{e:?}").as_str());
            }
        }
        Ok(())
    }

    async fn set_upstream_weight(
        self: Rc<Self>,
        params: site_group_control::SetUpstreamWeightParams,
        mut results: site_group_control::SetUpstreamWeightResults,
    ) -> capnp::Result<()> {
        let params = params.get()?;
        let site_id = params.get_site_id()?.to_str()?;
        let addr = params.get_addr()?.to_str()?;
        let weight = params.get_weight();
        let r = (|| {
            let addr = SocketAddr::from_str(addr)
                .map_err(|e| anyhow!("invalid upstream address {addr}: {e}"))?;
            self.site(site_id)?.set_upstream_weight(addr, weight)
        })();
        set_operation_result(results.get().init_result(), r);
        Ok(())
    }

    async fn list_upstream_health(
        self: Rc<Self>,
        params: site_group_control::ListUpstreamHealthParams,
        mut results: site_group_control::ListUpstreamHealthResults,
    ) -> capnp::Result<()> {
        let site_id = params.get()?.get_site_id()?.to_str()?;
        match self
            .site(site_id)
            .and_then(|site| site.list_upstream_health_status())
        {
            Ok(peers) => {
                let mut builder = results.get().init_result().init_peers(peers.len() as u32);
                for (i, peer) in peers.iter().enumerate() {
                    let mut item = builder.reborrow().get(i as u32);
                    item.set_addr(peer.addr.to_string().as_str());
                    item.set_fails(peer.fails);
                    item.set_unavailable(peer.unavailable);
                    let recover_in_ms = peer
                        .recover_in
                        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
                        .unwrap_or(0);
                    item.set_recover_in_ms(recover_in_ms);
                }
            }
            Err(e) => {
                let mut ev = results.get().init_result().init_err();
                ev.set_code(-1);
                ev.set_reason(format!("{e:?}").as_str());
            }
        }
        Ok(())
    }
}
