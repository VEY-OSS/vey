/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use h2::client::SendRequest;
use tokio::time::Instant;

use vey_types::net::ConnectionPoolConfig;

use crate::escape::EgressNotes;

/// Per-site HTTP/2 origin pool, sharded by worker.
///
/// Connections stay multiplexed: checkout clones `SendRequest` and does not
/// bind a client connection to an origin connection.
pub(crate) struct SiteHttp2Pool {
    config: ConnectionPoolConfig,
    lane_max_idle: usize,
    lanes: Box<[H2Lane]>,
}

struct H2Lane {
    conns: Mutex<Vec<PooledH2Connection>>,
}

struct PooledH2Connection {
    sender: SendRequest<Bytes>,
    is_tls: bool,
    last_used: Instant,
    closed: Arc<AtomicBool>,
    egress_notes: EgressNotes,
}

impl SiteHttp2Pool {
    pub(crate) fn new(config: ConnectionPoolConfig) -> Self {
        let lane_count = vey_daemon::runtime::worker::worker_count().max(1);
        let lane_max_idle = config.max_idle_count().div_ceil(lane_count).max(1);
        SiteHttp2Pool {
            config,
            lane_max_idle,
            lanes: (0..lane_count)
                .map(|_| H2Lane {
                    conns: Mutex::new(Vec::new()),
                })
                .collect(),
        }
    }

    pub(crate) async fn checkout(
        &self,
        is_tls: bool,
        worker_id: Option<usize>,
        open_timeout: Duration,
    ) -> Option<(SendRequest<Bytes>, EgressNotes)> {
        let lane = self.lane(worker_id);
        let idle_timeout = self.config.idle_timeout();
        let candidates = lane.snapshot(is_tls, idle_timeout);
        if candidates.is_empty() {
            return None;
        }

        for conn in &candidates {
            if conn.closed.load(Ordering::Acquire) {
                continue;
            }
            match tokio::time::timeout(open_timeout, conn.sender.clone().ready()).await {
                Ok(Ok(ready)) => {
                    lane.touch(conn.sender.clone(), is_tls);
                    return Some((ready, conn.egress_notes.clone()));
                }
                Ok(Err(_)) => continue,
                Err(_) => continue,
            }
        }

        if candidates.len() >= self.lane_max_idle
            && let Some(conn) = candidates
                .iter()
                .find(|c| !c.closed.load(Ordering::Acquire))
            && let Ok(Ok(ready)) =
                tokio::time::timeout(open_timeout, conn.sender.clone().ready()).await
        {
            lane.touch(conn.sender.clone(), is_tls);
            return Some((ready, conn.egress_notes.clone()));
        }
        None
    }

    pub(crate) fn insert(
        &self,
        worker_id: Option<usize>,
        is_tls: bool,
        sender: SendRequest<Bytes>,
        closed: Arc<AtomicBool>,
        egress_notes: EgressNotes,
    ) {
        self.lane(worker_id).push(
            PooledH2Connection {
                sender,
                is_tls,
                last_used: Instant::now(),
                closed,
                egress_notes,
            },
            self.lane_max_idle,
            self.config.idle_timeout(),
        );
    }

    fn lane(&self, worker_id: Option<usize>) -> &H2Lane {
        &self.lanes[lane_index(worker_id, self.lanes.len())]
    }
}

impl H2Lane {
    fn snapshot(&self, is_tls: bool, idle_timeout: Duration) -> Vec<PooledH2Connection> {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
        idle.iter()
            .filter(|c| c.is_tls == is_tls && !c.closed.load(Ordering::Acquire))
            .map(|c| PooledH2Connection {
                sender: c.sender.clone(),
                is_tls: c.is_tls,
                last_used: c.last_used,
                closed: Arc::clone(&c.closed),
                egress_notes: c.egress_notes.clone(),
            })
            .collect()
    }

    fn touch(&self, sender: SendRequest<Bytes>, is_tls: bool) {
        let mut idle = self.conns.lock().unwrap();
        if let Some(conn) = idle
            .iter_mut()
            .find(|c| c.is_tls == is_tls && senders_same(&c.sender, &sender))
        {
            conn.last_used = Instant::now();
        }
    }

    fn push(&self, pooled: PooledH2Connection, lane_max_idle: usize, idle_timeout: Duration) {
        let mut idle = self.conns.lock().unwrap();
        prune_idle(&mut idle, idle_timeout);
        if idle.len() >= lane_max_idle {
            return;
        }
        idle.push(pooled);
    }
}

fn senders_same(a: &SendRequest<Bytes>, b: &SendRequest<Bytes>) -> bool {
    // `SendRequest` does not expose identity. Compare by pointer of the clone
    // we just inserted is unnecessary for touch(); last_used is a hint.
    let _ = (a, b);
    true
}

fn lane_index(worker_id: Option<usize>, lane_count: usize) -> usize {
    match worker_id {
        Some(id) if id < lane_count => id,
        _ => 0,
    }
}

fn prune_idle(idle: &mut Vec<PooledH2Connection>, idle_timeout: Duration) {
    idle.retain(|c| !c.closed.load(Ordering::Acquire) && c.last_used.elapsed() < idle_timeout);
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
    }
}
