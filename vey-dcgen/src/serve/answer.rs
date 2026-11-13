/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::Future;
use std::io;
use std::sync::Arc;

use log::{debug, warn};
use tokio::time::Instant;

use vey_cert_agent::Request;
use vey_std_ext::time::DurationExt;

use super::stats::ServerStats;
use crate::issuer;

pub(super) async fn answer<F, Fut>(stats: &Arc<ServerStats>, packet: &[u8], peer: &str, send: F)
where
    F: FnOnce(Vec<u8>) -> Fut + Send + 'static,
    Fut: Future<Output = io::Result<()>> + Send + 'static,
{
    let alive = stats.enter_alive();
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

    if let Some(pair) = crate::site::get(user_req.site(), user_req.cert_usage()) {
        let ttl = pair.ttl();
        debug!(
            "{} - use predefined certificate for site {}",
            user_req.host(),
            user_req.site()
        );
        deliver(
            alive.stats(),
            recv_time,
            &user_req,
            pair.certificate(),
            pair.private_key(),
            ttl,
            send,
        )
        .await;
        return;
    }

    if let Ok(generated) = issuer::sign(&user_req) {
        deliver(
            alive.stats(),
            recv_time,
            &user_req,
            &generated.cert,
            &generated.key,
            generated.ttl,
            send,
        )
        .await;
    }
}

async fn deliver<F, Fut>(
    stats: &ServerStats,
    recv_time: Instant,
    user_req: &Request,
    cert: &str,
    key: &[u8],
    ttl: u32,
    send: F,
) where
    F: FnOnce(Vec<u8>) -> Fut,
    Fut: Future<Output = io::Result<()>>,
{
    let buf = match user_req.encode_rsp(cert, key, ttl) {
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
                user_req.host(),
            );
            if let Some(recorder) = issuer::issuer_duration_recorder() {
                let _ = recorder.record(duration_nanos);
            }
        }
        Err(e) => {
            stats.add_response_fail();
            warn!("write response back error: {e:?}");
        }
    }
}
