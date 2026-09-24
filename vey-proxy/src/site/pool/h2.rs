/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use ahash::AHashMap;
use bytes::Bytes;
use h2::client::SendRequest;
use tokio::time::{Instant, MissedTickBehavior};

use vey_types::metrics::NodeName;
use vey_types::net::ConnectionPoolConfig;

use super::{IsolationKey, lane_index};
use crate::escape::EgressNotes;

pub(crate) struct H2ConnectionState {
    closed: AtomicBool,
}

impl H2ConnectionState {
    pub(crate) fn new() -> Self {
        H2ConnectionState {
            closed: AtomicBool::new(false),
        }
    }

    pub(crate) fn mark_closed(&self) {
        self.closed.store(true, Ordering::Release);
    }

    pub(crate) fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

/// Per-site HTTP/2 origin pool, sharded by worker.
///
/// Checkout takes a connection out, then puts it back at the front when
/// `ready` succeeds so the next checkout uses another connection.
pub(crate) struct SiteHttp2Pool {
    config: ConnectionPoolConfig,
    lane_max_idle: usize,
    lanes: Box<[H2Lane]>,
}

#[derive(Default)]
struct H2Lane {
    pools: Mutex<AHashMap<IsolationKey, Arc<InnerPool>>>,
}

struct PooledH2Connection {
    sender: SendRequest<Bytes>,
    conn_state: Arc<H2ConnectionState>,
    egress_notes: EgressNotes,
    last_used: Instant,
}

struct InnerPool {
    conns: Mutex<VecDeque<PooledH2Connection>>,
}

impl SiteHttp2Pool {
    pub(crate) fn new(config: ConnectionPoolConfig) -> Self {
        let lane_count = vey_daemon::runtime::worker::worker_count().max(1);
        let lane_max_idle = config.max_idle_count().div_ceil(lane_count).max(1);
        SiteHttp2Pool {
            config,
            lane_max_idle,
            lanes: (0..lane_count).map(|_| H2Lane::default()).collect(),
        }
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

    pub(crate) async fn checkout(
        &self,
        worker_id: Option<usize>,
        escaper: &NodeName,
        peer: Option<SocketAddr>,
        open_timeout: Duration,
    ) -> Option<(SendRequest<Bytes>, EgressNotes)> {
        let inner_pool = self.lookup_inner_pool(worker_id, escaper, peer)?;
        let idle_timeout = self.config.idle_timeout();
        loop {
            let Some(mut conn) = inner_pool.pop_idle(idle_timeout) else {
                return None;
            };
            match tokio::time::timeout(open_timeout, conn.sender.clone().ready()).await {
                Ok(Ok(_)) => {
                    conn.last_used = Instant::now();
                    let sender = conn.sender.clone();
                    let egress_notes = conn.egress_notes.clone();
                    inner_pool.push_front(conn, self.lane_max_idle, idle_timeout);
                    return Some((sender, egress_notes));
                }
                Ok(Err(_)) => conn.conn_state.mark_closed(),
                Err(_) => {}
            }
        }
    }

    pub(crate) fn insert(
        &self,
        worker_id: Option<usize>,
        escaper: NodeName,
        peer: Option<SocketAddr>,
        sender: SendRequest<Bytes>,
        conn_state: Arc<H2ConnectionState>,
        egress_notes: EgressNotes,
    ) {
        if conn_state.is_closed() || egress_notes.is_expired() {
            return;
        }
        let inner_pool = self.get_inner_pool(worker_id, escaper, peer);
        inner_pool.push(
            PooledH2Connection {
                sender,
                conn_state,
                egress_notes,
                last_used: Instant::now(),
            },
            self.lane_max_idle,
            self.config.idle_timeout(),
        );
    }

    fn lane(&self, worker_id: Option<usize>) -> &H2Lane {
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

    fn pop_idle(&self, idle_timeout: Duration) -> Option<PooledH2Connection> {
        let mut conns = self.conns.lock().unwrap();
        prune_idle(&mut conns, idle_timeout);
        conns.pop_back()
    }

    fn push(&self, pooled: PooledH2Connection, max_idle: usize, idle_timeout: Duration) {
        let mut conns = self.conns.lock().unwrap();
        prune_idle(&mut conns, idle_timeout);
        if conns.len() >= max_idle {
            let _ = conns.pop_front();
        }
        conns.push_back(pooled);
    }

    fn push_front(&self, pooled: PooledH2Connection, max_idle: usize, idle_timeout: Duration) {
        let mut conns = self.conns.lock().unwrap();
        prune_idle(&mut conns, idle_timeout);
        if conns.len() >= max_idle {
            let _ = conns.pop_back();
        }
        conns.push_front(pooled);
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
        let mut conns = pool.conns.lock().unwrap();
        prune_idle(&mut conns, idle_timeout);
    }
}

impl PooledH2Connection {
    fn is_expired(&self, idle_timeout: Duration) -> bool {
        self.conn_state.is_closed()
            || self.egress_notes.is_expired()
            || self.last_used.elapsed() >= idle_timeout
    }
}

fn prune_idle(idle: &mut VecDeque<PooledH2Connection>, idle_timeout: Duration) {
    idle.retain(|c| !c.is_expired(idle_timeout));
}
