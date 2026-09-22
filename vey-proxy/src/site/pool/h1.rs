/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use vey_types::metrics::NodeName;
use vey_types::net::ConnectionPoolConfig;

use super::lane_index;
use crate::escape::EgressNotes;
use crate::module::http_forward::{
    BoxHttpForwardConnection, HttpAliveReuseNotes, HttpConnectionEofPoller,
};

/// Per-site HTTP/1 origin idle pool, sharded by worker.
///
/// Unaided workers are current-thread runtimes. A process-wide mutex would park
/// another worker's OS thread. Each worker only touches its own lane, so the
/// origin connection and its EOF poller stay on the runtime that opened them.
pub(crate) struct SiteHttp1Pool {
    config: ConnectionPoolConfig,
    lane_max_idle: usize,
    lanes: Box<[IdleLane]>,
}

struct IdleLane {
    conns: Mutex<VecDeque<PooledHttp1Connection>>,
}

struct PooledHttp1Connection {
    poller: HttpConnectionEofPoller,
    escaper: NodeName,
    peer: Option<SocketAddr>,
    reuse_notes: HttpAliveReuseNotes,
    egress_notes: EgressNotes,
    last_used: Instant,
}

impl SiteHttp1Pool {
    pub(crate) fn new(config: ConnectionPoolConfig) -> Self {
        let lane_count = vey_daemon::runtime::worker::worker_count().max(1);
        let lane_max_idle = config.max_idle_count().div_ceil(lane_count).max(1);
        SiteHttp1Pool {
            config,
            lane_max_idle,
            lanes: (0..lane_count)
                .map(|_| IdleLane {
                    conns: Mutex::new(VecDeque::new()),
                })
                .collect(),
        }
    }

    pub(crate) async fn get(
        &self,
        worker_id: Option<usize>,
        escaper: &NodeName,
        peer: Option<SocketAddr>,
    ) -> Option<(BoxHttpForwardConnection, HttpAliveReuseNotes, EgressNotes)> {
        let lane = self.lane(worker_id);
        let idle_timeout = self.config.idle_timeout();
        loop {
            let mut conn = lane.pop_candidate(escaper, peer, idle_timeout)?;
            conn.reuse_notes.keep_alive_leftover.decrement_max_mut();
            let reuse_notes = conn.reuse_notes;
            let egress_notes = conn.egress_notes;
            if let Some(connection) = conn.poller.recv_conn().await {
                return Some((connection, reuse_notes, egress_notes));
            }
        }
    }

    pub(crate) fn save(
        &self,
        worker_id: Option<usize>,
        escaper: NodeName,
        peer: Option<SocketAddr>,
        connection: BoxHttpForwardConnection,
        reuse_notes: HttpAliveReuseNotes,
        egress_notes: EgressNotes,
    ) {
        let idle_timeout = self.config.idle_timeout();
        if reuse_notes.is_exhausted() || idle_timeout.is_zero() || egress_notes.is_expired() {
            return;
        }

        let pooled = PooledHttp1Connection {
            poller: HttpConnectionEofPoller::spawn(connection),
            escaper,
            peer,
            reuse_notes,
            egress_notes,
            last_used: Instant::now(),
        };
        self.lane(worker_id)
            .push(pooled, self.lane_max_idle, idle_timeout);
    }

    fn lane(&self, worker_id: Option<usize>) -> &IdleLane {
        &self.lanes[lane_index(worker_id, self.lanes.len())]
    }
}

impl IdleLane {
    fn pop_candidate(
        &self,
        escaper: &NodeName,
        peer: Option<SocketAddr>,
        idle_timeout: Duration,
    ) -> Option<PooledHttp1Connection> {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
        let pos = idle.iter().rposition(|c| {
            &c.escaper == escaper && c.peer == peer && !c.is_expired(idle_timeout)
        })?;
        idle.remove(pos)
    }

    fn push(&self, pooled: PooledHttp1Connection, lane_max_idle: usize, idle_timeout: Duration) {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
        if idle.len() >= lane_max_idle {
            let _ = idle.pop_front();
        }
        if idle.len() >= lane_max_idle {
            return;
        }
        idle.push_back(pooled);
    }
}

impl PooledHttp1Connection {
    fn is_expired(&self, idle_timeout: Duration) -> bool {
        self.poller.is_closed()
            || self.reuse_notes.is_exhausted()
            || self.egress_notes.is_expired()
            || self.last_used.elapsed() >= idle_timeout
    }
}

fn prune_idle(idle: &mut VecDeque<PooledHttp1Connection>, idle_timeout: Duration) {
    while idle.back().is_some_and(|c| c.is_expired(idle_timeout)) {
        idle.pop_back();
    }
    while idle.front().is_some_and(|c| c.is_expired(idle_timeout)) {
        idle.pop_front();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn lane_max_idle_splits_site_cap() {
        assert_eq!(16usize.div_ceil(8).max(1), 2);
        assert_eq!(16usize.div_ceil(32).max(1), 1);
        assert_eq!(1usize.div_ceil(8).max(1), 1);
    }
}
