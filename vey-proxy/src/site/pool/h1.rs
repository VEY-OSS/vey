/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use ahash::AHashMap;
use tokio::time::{Instant, MissedTickBehavior};

use vey_types::metrics::NodeName;
use vey_types::net::ConnectionPoolConfig;

use super::{lane_index, IsolationKey};
use crate::escape::EgressNotes;
use crate::module::http_forward::{
    BoxHttpForwardConnection, HttpAliveReuseNotes, HttpConnectionEofPoller,
};

/// Per-site HTTP/1 origin idle pool, sharded by worker.
///
/// Unaided workers are current-thread runtimes. A process-wide mutex would park
/// another worker's OS thread. Each worker only touches its own lane, so the
/// origin connection and its EOF poller stay on the runtime that opened them.
/// Inside a lane, connections are split by escaper and peer.
pub(crate) struct SiteHttp1Pool {
    config: ConnectionPoolConfig,
    lane_max_idle: usize,
    lanes: Box<[H1Lane]>,
}

#[derive(Default)]
struct H1Lane {
    pools: Mutex<AHashMap<IsolationKey, Arc<InnerPool>>>,
}

struct PooledHttp1Connection {
    poller: HttpConnectionEofPoller,
    reuse_notes: HttpAliveReuseNotes,
    egress_notes: EgressNotes,
    last_used: Instant,
}

struct InnerPool {
    conns: Mutex<VecDeque<PooledHttp1Connection>>,
}

impl SiteHttp1Pool {
    pub(crate) fn new(config: ConnectionPoolConfig) -> Self {
        let lane_count = vey_daemon::runtime::worker::worker_count().max(1);
        let lane_max_idle = config.max_idle_count().div_ceil(lane_count).max(1);
        SiteHttp1Pool {
            config,
            lane_max_idle,
            lanes: (0..lane_count).map(|_| H1Lane::default()).collect(),
        }
    }

    pub(crate) async fn get(
        &self,
        worker_id: Option<usize>,
        escaper: &NodeName,
        peer: Option<SocketAddr>,
    ) -> Option<(BoxHttpForwardConnection, HttpAliveReuseNotes, EgressNotes)> {
        let inner_pool = self.lookup_inner_pool(worker_id, escaper, peer)?;
        let idle_timeout = self.config.idle_timeout();
        loop {
            let mut conn = inner_pool.pop_candidate(idle_timeout)?;
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

        let inner_pool = self.get_inner_pool(worker_id, escaper, peer);
        inner_pool.push(
            PooledHttp1Connection {
                poller: HttpConnectionEofPoller::spawn(connection),
                reuse_notes,
                egress_notes,
                last_used: Instant::now(),
            },
            self.lane_max_idle,
            idle_timeout,
        );
    }

    fn get_inner_pool(
        &self,
        worker_id: Option<usize>,
        escaper: NodeName,
        peer_addr: Option<SocketAddr>,
    ) -> Arc<InnerPool> {
        let lane = self.lane(worker_id);
        let key = IsolationKey { escaper, peer_addr };
        let mut pools = lane.pools.lock().unwrap();
        if let Some(pool) = pools.get(&key) {
            return Arc::clone(pool);
        }
        let pool = InnerPool::spawn(self.config.idle_timeout(), self.config.check_interval());
        pools.insert(key, Arc::clone(&pool));
        pool
    }

    fn lookup_inner_pool(
        &self,
        worker_id: Option<usize>,
        escaper: &NodeName,
        peer_addr: Option<SocketAddr>,
    ) -> Option<Arc<InnerPool>> {
        let key = IsolationKey {
            escaper: escaper.clone(),
            peer_addr,
        };
        self.lane(worker_id)
            .pools
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
    }

    fn lane(&self, worker_id: Option<usize>) -> &H1Lane {
        &self.lanes[lane_index(worker_id, self.lanes.len())]
    }
}

impl InnerPool {
    fn spawn(idle_timeout: Duration, check_interval: Duration) -> Arc<Self> {
        let pool = Arc::new(InnerPool {
            conns: Mutex::new(VecDeque::new()),
        });
        if check_interval.is_zero() {
            return pool;
        }
        let weak = Arc::downgrade(&pool);
        tokio::spawn(sweep_idle(weak, idle_timeout, check_interval));
        pool
    }

    fn pop_candidate(&self, idle_timeout: Duration) -> Option<PooledHttp1Connection> {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
        let pos = idle.iter().rposition(|c| !c.is_expired(idle_timeout))?;
        idle.remove(pos)
    }

    fn push(&self, pooled: PooledHttp1Connection, max_idle: usize, idle_timeout: Duration) {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
        if idle.len() >= max_idle {
            let _ = idle.pop_front();
        }
        if idle.len() >= max_idle {
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

async fn sweep_idle(pool: Weak<InnerPool>, idle_timeout: Duration, check_interval: Duration) {
    let mut interval = tokio::time::interval(check_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        interval.tick().await;
        let Some(pool) = pool.upgrade() else {
            break;
        };
        let mut idle = pool.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
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
