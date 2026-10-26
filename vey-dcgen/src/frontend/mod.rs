/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use log::{debug, warn};
use tokio::time::Instant;

use vey_cert_agent::Request;
use vey_daemon::listen::{RespondUdpServer, UdpResponder};
use vey_daemon::server::{BaseServer, ReloadServer};
use vey_histogram::HistogramRecorder;
use vey_types::metrics::NodeName;

use crate::{BackendRequest, BackendResponse};

mod stats;
pub(crate) use stats::FrontendStats;

#[derive(Debug)]
pub(crate) struct GeneratedData {
    pub(crate) cert: String,
    pub(crate) key: Vec<u8>,
    pub(crate) ttl: u32,
}

#[derive(Clone)]
pub(super) struct Frontend {
    name: NodeName,
    stats: Arc<FrontendStats>,
    req_sender: kanal::AsyncSender<BackendRequest>,
}

impl Frontend {
    pub(super) fn new(
        stats: Arc<FrontendStats>,
        req_sender: kanal::AsyncSender<BackendRequest>,
    ) -> Self {
        Frontend {
            name: NodeName::new_static("dcgen"),
            stats,
            req_sender,
        }
    }

    pub(super) fn stats(&self) -> Arc<FrontendStats> {
        self.stats.clone()
    }
}

impl BaseServer for Frontend {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn r#type(&self) -> &'static str {
        "dcgen"
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
        responder: &'a UdpResponder,
        worker_id: Option<usize>,
    ) -> impl Future<Output = ()> + Send + 'a {
        async move {
            let _ = (server_addr, worker_id);
            self.stats.add_request_total();
            let recv_time = Instant::now();
            match Request::parse_req(packet) {
                Ok(user_req) => {
                    debug!("{} - request received", user_req.host());
                    let req = BackendRequest {
                        user_req,
                        peer: client_addr,
                        responder: responder.clone(),
                        recv_time,
                    };
                    if let Err(e) = self.req_sender.send(req).await {
                        warn!("failed to send request to backend: {e}");
                    }
                }
                Err(e) => {
                    self.stats.add_request_invalid();
                    warn!("invalid request from client {client_addr}: {e:?}");
                }
            }
        }
    }
}

pub(super) async fn send_response(
    stats: &FrontendStats,
    duration_recorder: &HistogramRecorder<u64>,
    rsp: BackendResponse,
) {
    match rsp
        .user_req
        .encode_rsp(&rsp.generated.cert, &rsp.generated.key, rsp.generated.ttl)
    {
        Ok(buf) => {
            stats.add_response_total();
            match rsp.responder.send_to(buf.as_slice(), rsp.peer).await {
                Ok(_) => {
                    let duration_nanos = rsp.duration();
                    debug!(
                        "{} - duration: {}ns, rsp size: {}",
                        rsp.user_req.host(),
                        duration_nanos,
                        buf.len()
                    );
                    let _ = duration_recorder.record(duration_nanos);
                }
                Err(e) => {
                    stats.add_response_fail();
                    warn!("write response back to {} error: {e:?}", rsp.peer);
                }
            }
        }
        Err(e) => warn!("response encode error: {e:?}"),
    }
}
