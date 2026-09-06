/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use vey_types::net::ConnectionPoolConfig;

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
    saved_at: Instant,
    poller: HttpConnectionEofPoller,
    is_tls: bool,
    reuse_notes: HttpAliveReuseNotes,
    egress_notes: EgressNotes,
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
        idle_expire: Duration,
        is_tls: bool,
        worker_id: Option<usize>,
    ) -> Option<(BoxHttpForwardConnection, HttpAliveReuseNotes, EgressNotes)> {
        let idle_expire = idle_expire.min(self.config.idle_timeout());
        let lane = self.lane(worker_id);
        loop {
            let mut conn = lane.pop_candidate(idle_expire, is_tls)?;
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
        is_tls: bool,
        connection: BoxHttpForwardConnection,
        reuse_notes: HttpAliveReuseNotes,
        egress_notes: EgressNotes,
    ) {
        if reuse_notes.keep_alive_leftover.max() == Some(0) {
            return;
        }

        let pooled = PooledHttp1Connection {
            saved_at: Instant::now(),
            poller: HttpConnectionEofPoller::spawn(connection),
            is_tls,
            reuse_notes,
            egress_notes,
        };
        self.lane(worker_id)
            .push(pooled, self.lane_max_idle, self.config.idle_timeout());
    }

    fn lane(&self, worker_id: Option<usize>) -> &IdleLane {
        &self.lanes[lane_index(worker_id, self.lanes.len())]
    }
}

impl IdleLane {
    fn pop_candidate(&self, idle_expire: Duration, is_tls: bool) -> Option<PooledHttp1Connection> {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_expire);
        loop {
            let conn = idle.pop_back()?;
            if conn.is_tls != is_tls || conn.is_expired(idle_expire) {
                continue;
            }
            return Some(conn);
        }
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
    fn is_expired(&self, idle_expire: Duration) -> bool {
        let keep_alive = self.reuse_notes.keep_alive_leftover;
        if self.poller.is_closed() || keep_alive.max() == Some(0) {
            return true;
        }
        let timeout = keep_alive
            .timeout()
            .map(|t| t.min(idle_expire))
            .unwrap_or(idle_expire);
        self.saved_at.elapsed() >= timeout
    }
}

fn lane_index(worker_id: Option<usize>, lane_count: usize) -> usize {
    match worker_id {
        Some(id) if id < lane_count => id,
        _ => 0,
    }
}

fn prune_idle(idle: &mut VecDeque<PooledHttp1Connection>, idle_expire: Duration) {
    while idle.back().is_some_and(|c| c.is_expired(idle_expire)) {
        idle.pop_back();
    }
    while idle.front().is_some_and(|c| c.is_expired(idle_expire)) {
        idle.pop_front();
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

    #[test]
    fn lane_max_idle_splits_site_cap() {
        assert_eq!(16usize.div_ceil(8).max(1), 2);
        assert_eq!(16usize.div_ceil(32).max(1), 1);
        assert_eq!(1usize.div_ceil(8).max(1), 1);
    }
}
