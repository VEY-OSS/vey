/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use log::warn;
use tokio::time::{Interval, MissedTickBehavior};

use super::{IcapClientConnection, IcapConnector, IcapServiceConfig};
use crate::options::{IcapOptionsRequest, IcapServiceOptions};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod bench;

struct IdleIcapConnection {
    conn: IcapClientConnection,
    idle_since: Instant,
}

struct IcapLane {
    idle: Mutex<VecDeque<IdleIcapConnection>>,
    maintainer_started: AtomicBool,
}

/// ICAP idle pool, sharded by unaided worker.
///
/// Unaided workers are current-thread runtimes. A process-wide mutex would park
/// another worker's OS thread, and a socket opened on one runtime cannot be
/// polled on another. Each worker only touches its own lane. The lane
/// maintainer is spawned on that worker, so refill and idle expiry stay there.
pub(super) struct IcapConnectionPool {
    config: Arc<IcapServiceConfig>,
    options: ArcSwap<IcapServiceOptions>,
    lanes: Box<[IcapLane]>,
    lane_max_idle: usize,
    lane_min_idle: usize,
    connector: Arc<IcapConnector>,
}

/// Out-of-range and missing ids share lane 0.
fn lane_index(worker_id: Option<usize>, lane_count: usize) -> usize {
    match worker_id {
        Some(id) if id < lane_count => id,
        _ => 0,
    }
}

/// Split a process-wide idle cap across lanes. Zero stays zero so a disabled
/// pool does not start holding connections. A positive cap is at least one per
/// lane, so every worker can reuse a connection.
fn per_lane(total: usize, lane_count: usize) -> usize {
    if total == 0 {
        0
    } else {
        total.div_ceil(lane_count).max(1)
    }
}

impl IcapConnectionPool {
    pub(super) fn new(
        config: Arc<IcapServiceConfig>,
        connector: Arc<IcapConnector>,
        lane_count: usize,
    ) -> Self {
        let lane_count = lane_count.max(1);
        let lane_min_idle = per_lane(config.connection_pool.min_idle_count(), lane_count);
        let lane_max_idle = per_lane(config.connection_pool.max_idle_count(), lane_count);
        let lanes = (0..lane_count)
            .map(|_| IcapLane {
                idle: Mutex::new(VecDeque::with_capacity(lane_min_idle)),
                maintainer_started: AtomicBool::new(false),
            })
            .collect();

        let options = ArcSwap::new(Arc::new(IcapServiceOptions::new_expired(config.method)));

        IcapConnectionPool {
            config,
            options,
            lanes,
            lane_max_idle,
            lane_min_idle,
            connector,
        }
    }

    /// Start this lane's maintainer on the current runtime. Call this from the
    /// worker that owns the lane, so refill and idle expiry stay there.
    pub(super) fn ensure_lane(self: &Arc<Self>, lane: usize) {
        let period = self.config.connection_pool.check_interval();
        if period.is_zero() {
            return;
        }
        let lane = lane.min(self.lanes.len() - 1);
        if self.lanes[lane]
            .maintainer_started
            .swap(true, Ordering::AcqRel)
        {
            return;
        }
        let maintainer = PoolMaintainer::new(Arc::downgrade(self), lane, period);
        tokio::spawn(maintainer.into_running());
    }

    pub(super) fn ensure_worker(self: &Arc<Self>, worker_id: Option<usize>) {
        self.ensure_lane(lane_index(worker_id, self.lanes.len()));
    }

    pub fn try_put(&self, conn: IcapClientConnection) -> bool {
        if !conn.reusable() {
            return false;
        }
        let lane = conn.lane().min(self.lanes.len() - 1);
        let mut idle = self.lanes[lane].idle.lock().unwrap();
        if idle.len() >= self.lane_max_idle {
            return false;
        }
        idle.push_back(IdleIcapConnection {
            conn,
            idle_since: Instant::now(),
        });
        true
    }

    fn take(&self, lane: usize) -> Option<IcapClientConnection> {
        self.lanes[lane]
            .idle
            .lock()
            .unwrap()
            .pop_back()
            .map(|i| i.conn)
    }

    pub async fn get(&self, worker_id: Option<usize>) -> io::Result<IcapClientConnection> {
        let lane = lane_index(worker_id, self.lanes.len());
        while let Some(mut conn) = self.take(lane) {
            if !conn.probe_idle().await {
                continue;
            }
            if self.options.load().expired() {
                match self.handshake(conn).await {
                    Ok(mut conn) => {
                        conn.mark_reused();
                        return Ok(conn);
                    }
                    Err(e) => {
                        warn!("icap OPTIONS on idle connection failed: {e}");
                        continue;
                    }
                }
            }
            conn.mark_reused();
            return Ok(conn);
        }
        let mut conn = self.connector.create().await?;
        conn.set_lane(lane);
        if self.options.load().expired() {
            self.handshake(conn).await
        } else {
            Ok(conn)
        }
    }

    fn expire(&self, lane: usize) -> usize {
        let now = Instant::now();
        let idle_timeout = self.config.connection_pool.idle_timeout();

        let mut expired = Vec::new();
        {
            let mut pool = self.lanes[lane].idle.lock().unwrap();
            while let Some(conn) =
                pool.pop_front_if(|c| now.duration_since(c.idle_since) >= idle_timeout)
            {
                expired.push(conn);
            }
        }
        expired.len()
    }

    async fn handshake(&self, mut conn: IcapClientConnection) -> io::Result<IcapClientConnection> {
        conn.mark_io_inuse();
        let req = IcapOptionsRequest::new(&self.config);
        let fut = req.get_options(&mut conn, self.config.icap_max_header_size);
        match tokio::time::timeout(self.config.options_timeout, fut).await {
            Ok(Ok(options)) => {
                self.options.store(Arc::new(options));
                Ok(conn)
            }
            Ok(Err(e)) => {
                warn!("icap options request failed: {e}");
                Err(io::Error::other(e))
            }
            Err(_) => {
                let msg = format!(
                    "icap options request timed out after {:?}",
                    self.config.options_timeout
                );
                warn!("{msg}");
                Err(io::Error::new(io::ErrorKind::TimedOut, msg))
            }
        }
    }

    async fn refill(&self, lane: usize) {
        let pool_size = self.lanes[lane].idle.lock().unwrap().len();
        for _ in 0..self.lane_min_idle.saturating_sub(pool_size) {
            match self.connector.create().await {
                Ok(mut conn) => {
                    conn.set_lane(lane);
                    if !self.try_put(conn) {
                        break;
                    }
                }
                Err(e) => {
                    warn!("failed to refill ICAP connection pool: {e}");
                    break;
                }
            }
        }
    }

    pub fn get_options(&self) -> Arc<IcapServiceOptions> {
        self.options.load_full()
    }

    async fn check_options(&self, lane: usize) {
        if !self.options.load().expired() {
            return;
        }

        // OPTIONS uses a dedicated connection so a timeout or handshake
        // failure cannot steal idle connections from get().
        match self.connector.create().await {
            Ok(mut conn) => {
                conn.set_lane(lane);
                if let Ok(conn) = self.handshake(conn).await {
                    self.try_put(conn);
                }
            }
            Err(e) => warn!("icap options connect failed: {e}"),
        }
    }
}

pub(super) struct PoolMaintainer {
    pool: Weak<IcapConnectionPool>,
    lane: usize,
    check_interval: Interval,
}

impl PoolMaintainer {
    pub(super) fn new(pool: Weak<IcapConnectionPool>, lane: usize, period: Duration) -> Self {
        let mut check_interval = tokio::time::interval(period);
        check_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        PoolMaintainer {
            pool,
            lane,
            check_interval,
        }
    }

    pub(super) async fn into_running(mut self) {
        loop {
            self.check_interval.tick().await;

            let Some(pool) = self.pool.upgrade() else {
                return;
            };

            pool.expire(self.lane);
            pool.check_options(self.lane).await;
            pool.refill(self.lane).await;
        }
    }
}
