/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use vey_types::net::{ConnectionPoolConfig, KeepAliveValue};

use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::http_forward::{BoxHttpForwardConnection, HttpConnectionEofPoller};

pub(crate) struct SiteHttp1Pool {
    config: ConnectionPoolConfig,
    idle: Mutex<VecDeque<PooledHttp1Connection>>,
}

struct PooledHttp1Connection {
    saved_at: Instant,
    poller: HttpConnectionEofPoller,
    keep_alive: KeepAliveValue,
    is_tls: bool,
    escaper: ArcEscaper,
    egress_notes: EgressNotes,
}

impl SiteHttp1Pool {
    pub(crate) fn new(config: ConnectionPoolConfig) -> Self {
        SiteHttp1Pool {
            config,
            idle: Mutex::new(VecDeque::new()),
        }
    }

    pub(crate) async fn get(
        &self,
        idle_expire: Duration,
        is_tls: bool,
    ) -> Option<(
        BoxHttpForwardConnection,
        KeepAliveValue,
        ArcEscaper,
        EgressNotes,
    )> {
        let idle_expire = idle_expire.min(self.config.idle_timeout());
        loop {
            let conn = self.pop_candidate(idle_expire, is_tls)?;
            let leftover = conn.keep_alive.decrement_max();
            let escaper = conn.escaper;
            let egress_notes = conn.egress_notes;
            if let Some(c) = conn.poller.recv_conn().await {
                return Some((c, leftover, escaper, egress_notes));
            }
        }
    }

    pub(crate) fn save(
        &self,
        c: BoxHttpForwardConnection,
        ka: KeepAliveValue,
        leftover: Option<KeepAliveValue>,
        is_tls: bool,
        escaper: ArcEscaper,
        egress_notes: EgressNotes,
    ) {
        let keep_alive = ka.or_from(leftover.unwrap_or_default());
        if keep_alive.max() == Some(0) {
            return;
        }

        let pooled = PooledHttp1Connection {
            saved_at: Instant::now(),
            poller: HttpConnectionEofPoller::spawn(c),
            keep_alive,
            is_tls,
            escaper,
            egress_notes,
        };

        let mut idle = self.idle.lock().unwrap();
        prune_idle(&mut idle, self.config.idle_timeout());
        if idle.len() >= self.config.max_idle_count() {
            let _ = idle.pop_front();
        }
        if idle.len() >= self.config.max_idle_count() {
            return;
        }
        idle.push_back(pooled);
    }

    fn pop_candidate(&self, idle_expire: Duration, is_tls: bool) -> Option<PooledHttp1Connection> {
        let mut idle = self.idle.lock().unwrap();
        prune_idle(&mut idle, idle_expire);
        loop {
            let conn = idle.pop_back()?;
            if conn.is_tls != is_tls || conn.is_expired(idle_expire) {
                continue;
            }
            return Some(conn);
        }
    }
}

impl PooledHttp1Connection {
    fn is_expired(&self, idle_expire: Duration) -> bool {
        if self.poller.is_closed() || self.keep_alive.max() == Some(0) {
            return true;
        }
        let timeout = self
            .keep_alive
            .timeout()
            .map(|t| t.min(idle_expire))
            .unwrap_or(idle_expire);
        self.saved_at.elapsed() >= timeout
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
