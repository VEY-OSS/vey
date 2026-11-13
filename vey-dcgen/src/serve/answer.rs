/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use log::{debug, warn};

use vey_cert_agent::Request;

use super::stats::ServerStats;
use crate::issuer;

pub(super) fn answer(stats: &ServerStats, packet: &[u8], peer: &str) -> Option<Vec<u8>> {
    stats.add_request_total();

    let user_req = match Request::parse_req(packet) {
        Ok(user_req) => user_req,
        Err(e) => {
            stats.add_request_invalid();
            warn!("invalid request from peer {peer}: {e:?}");
            return None;
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
        return encode(
            &user_req,
            pair.certificate(),
            pair.private_key(),
            ttl,
            stats,
        );
    }

    let generated = issuer::sign(&user_req).ok()?;
    encode(
        &user_req,
        &generated.cert,
        &generated.key,
        generated.ttl,
        stats,
    )
}

fn encode(
    user_req: &Request,
    cert: &str,
    key: &[u8],
    ttl: u32,
    stats: &ServerStats,
) -> Option<Vec<u8>> {
    match user_req.encode_rsp(cert, key, ttl) {
        Ok(buf) => {
            stats.add_response_total();
            Some(buf)
        }
        Err(e) => {
            warn!("response encode error: {e:?}");
            None
        }
    }
}
