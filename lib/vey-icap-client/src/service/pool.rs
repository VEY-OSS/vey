/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::io;
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

pub(super) struct IcapConnectionPool {
    config: Arc<IcapServiceConfig>,
    options: ArcSwap<IcapServiceOptions>,
    idle_pool: Mutex<VecDeque<IdleIcapConnection>>,
    connector: Arc<IcapConnector>,
}

impl IcapConnectionPool {
    pub(super) fn new(config: Arc<IcapServiceConfig>, connector: Arc<IcapConnector>) -> Self {
        let idle_pool = Mutex::new(VecDeque::with_capacity(
            config.connection_pool.min_idle_count(),
        ));

        let options = ArcSwap::new(Arc::new(IcapServiceOptions::new_expired(config.method)));

        IcapConnectionPool {
            config,
            options,
            idle_pool,
            connector,
        }
    }

    pub fn try_put(&self, conn: IcapClientConnection) -> bool {
        if !conn.reusable() {
            return false;
        }
        let mut idle = self.idle_pool.lock().unwrap();
        if idle.len() >= self.config.connection_pool.max_idle_count() {
            return false;
        }
        idle.push_back(IdleIcapConnection {
            conn,
            idle_since: Instant::now(),
        });
        true
    }

    fn take(&self) -> Option<IcapClientConnection> {
        self.idle_pool.lock().unwrap().pop_back().map(|i| i.conn)
    }

    pub async fn get(&self) -> io::Result<IcapClientConnection> {
        while let Some(mut conn) = self.take() {
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
        let conn = self.connector.create().await?;
        if self.options.load().expired() {
            self.handshake(conn).await
        } else {
            Ok(conn)
        }
    }

    fn expire(&self) -> usize {
        let now = Instant::now();
        let idle_timeout = self.config.connection_pool.idle_timeout();

        let mut expired = Vec::new();
        {
            let mut pool = self.idle_pool.lock().unwrap();
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

    async fn refill(&self) {
        let pool_size = self.idle_pool.lock().unwrap().len();
        let min_idle = self.config.connection_pool.min_idle_count();
        for _ in 0..min_idle.saturating_sub(pool_size) {
            match self.connector.create().await {
                Ok(conn) => {
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

    async fn check_options(&self) {
        if !self.options.load().expired() {
            return;
        }

        // OPTIONS uses a dedicated connection so a timeout or handshake
        // failure cannot steal idle connections from get().
        match self.connector.create().await {
            Ok(conn) => {
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
    check_interval: Interval,
}

impl PoolMaintainer {
    pub(super) fn new(pool: Weak<IcapConnectionPool>, period: Duration) -> Self {
        let mut check_interval = tokio::time::interval(period);
        check_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        PoolMaintainer {
            pool,
            check_interval,
        }
    }

    pub(super) async fn into_running(mut self) {
        loop {
            self.check_interval.tick().await;

            let Some(pool) = self.pool.upgrade() else {
                return;
            };

            pool.expire();
            pool.check_options().await;
            pool.refill().await;
        }
    }
}
