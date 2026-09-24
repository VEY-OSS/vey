/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;

use vey_types::metrics::NodeName;

mod h1;
pub(crate) use h1::SiteHttp1Pool;

mod h2;
pub(crate) use h2::{H2ConnectionState, SiteHttp2Pool};

#[derive(Hash, Eq, PartialEq)]
struct IsolationKey {
    escaper: NodeName,
    peer_addr: Option<SocketAddr>,
}

/// Pick the unaided-worker lane. Out-of-range and missing ids use lane 0.
fn lane_index(worker_id: Option<usize>, lane_count: usize) -> usize {
    match worker_id {
        Some(id) if id < lane_count => id,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lane_stays_on_worker_id() {
        assert_eq!(lane_index(Some(0), 4), 0);
        assert_eq!(lane_index(Some(3), 4), 3);
        assert_eq!(lane_index(Some(4), 4), 0);
        assert_eq!(lane_index(None, 4), 0);
        assert_eq!(lane_index(None, 1), 0);
    }
}
