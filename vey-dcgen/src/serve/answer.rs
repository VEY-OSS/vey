/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::Future;
use std::io;

use log::{debug, warn};
use tokio::time::Instant;

use vey_cert_agent::Request;
use vey_std_ext::time::DurationExt;

use super::stats::ServerStats;
use crate::issuer;

pub(super) async fn answer<F, Fut>(stats: &ServerStats, packet: &[u8], peer: &str, send: F)
where
    F: FnOnce(Vec<u8>) -> Fut,
    Fut: Future<Output = io::Result<()>>,
{
    let _alive = stats.enter_alive();
    stats.add_request_total();
    let recv_time = Instant::now();

    let user_req = match Request::parse_req(packet) {
        Ok(user_req) => user_req,
        Err(e) => {
            stats.add_request_invalid();
            warn!("invalid request from peer {peer}: {e:?}");
            return;
        }
    };
    debug!("{} - request received", user_req.host());

    let generated = match issuer::generate(user_req).await {
        Ok(generated) => generated,
        Err(issuer::GenerateError::Failed) => return,
        Err(issuer::GenerateError::Dropped) => {
            warn!("issuer dropped the request");
            return;
        }
        Err(issuer::GenerateError::Queue(e)) => {
            warn!("{e}");
            return;
        }
    };

    let buf = match generated
        .user_req
        .encode_rsp(&generated.cert, &generated.key, generated.ttl)
    {
        Ok(buf) => buf,
        Err(e) => {
            warn!("response encode error: {e:?}");
            return;
        }
    };

    stats.add_response_total();
    let rsp_len = buf.len();
    match send(buf).await {
        Ok(()) => {
            let duration_nanos = recv_time.elapsed().as_nanos_u64();
            debug!(
                "{} - duration: {duration_nanos}ns, rsp size: {rsp_len}",
                generated.user_req.host(),
            );
            let _ = issuer::duration_recorder().record(duration_nanos);
        }
        Err(e) => {
            stats.add_response_fail();
            warn!("write response back error: {e:?}");
        }
    }
}
