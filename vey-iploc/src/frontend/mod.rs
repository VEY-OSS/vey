/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use log::warn;

use vey_daemon::listen::{RespondUdpServer, UdpResponder};
use vey_daemon::server::{BaseServer, ReloadServer};
use vey_geoip_types::{IpLocation, IpLocationBuilder};
use vey_ip_locate::{Request, Response};
use vey_types::metrics::NodeName;

mod stats;
pub(crate) use stats::FrontendStats;

#[derive(Clone)]
pub(super) struct Frontend {
    name: NodeName,
    stats: Arc<FrontendStats>,
}

impl Frontend {
    pub(super) fn new(stats: Arc<FrontendStats>) -> Self {
        Frontend {
            name: NodeName::new_static("iploc"),
            stats,
        }
    }

    fn fetch(&self, ip: IpAddr) -> Option<IpLocation> {
        let mut builder = IpLocationBuilder::default();

        if let Some(db) = vey_geoip_db::store::load_country()
            && let Some((net, v)) = db.longest_match(ip)
        {
            builder.set_network(net);
            builder.set_country(v.country);
            builder.set_continent(v.continent);
        }

        if let Some(asn_db) = vey_geoip_db::store::load_asn()
            && let Some((net, v)) = asn_db.longest_match(ip)
        {
            builder.set_network(net);
            builder.set_as_number(v.number);
            if let Some(name) = v.isp_name() {
                builder.set_isp_name(name.to_owned());
            }
            if let Some(domain) = v.isp_domain() {
                builder.set_isp_domain(domain.to_owned());
            }
        }

        builder.build().ok()
    }
}

impl BaseServer for Frontend {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn r#type(&self) -> &'static str {
        "iploc"
    }

    fn version(&self) -> usize {
        1
    }
}

impl ReloadServer for Frontend {
    fn reload(&self) -> Self {
        self.clone()
    }
}

impl RespondUdpServer for Frontend {
    fn handle_udp_request<'a>(
        &'a self,
        packet: &'a [u8],
        client_addr: SocketAddr,
        server_addr: SocketAddr,
        responder: UdpResponder,
        worker_id: Option<usize>,
    ) -> impl Future<Output = ()> + Send + 'a {
        async move {
            let _ = (server_addr, worker_id);
            self.stats.add_request_total();

            let req = match Request::parse_req(packet) {
                Ok(req) => req,
                Err(e) => {
                    self.stats.add_request_invalid();
                    warn!("invalid request from {client_addr}: {e:?}");
                    return;
                }
            };
            let Some(ip) = req.ip() else {
                self.stats.add_request_invalid();
                return;
            };

            let Some(location) = self.fetch(ip) else {
                return;
            };

            match Response::encode_new(ip, location, 300) {
                Ok(buf) => {
                    self.stats.add_response_total();
                    if responder.send(&buf).await.is_err() {
                        self.stats.add_response_fail();
                    }
                }
                Err(e) => {
                    warn!("failed to encode response for ip {ip}: {e}");
                }
            }
        }
    }
}
