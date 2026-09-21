/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;

use vey_types::net::{HttpForwardCapability, KeepAliveValue, UpstreamAddr};

use super::{ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpConnectionEofPoller};
use crate::audit::AuditContext;
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;

mod direct;
pub(crate) use direct::DirectHttpForwardContext;

mod proxy;
pub(crate) use proxy::ProxyHttpForwardContext;

mod route;
pub(crate) use route::RouteHttpForwardContext;

mod failover;
pub(crate) use failover::FailoverHttpForwardContext;

pub(crate) type BoxHttpForwardContext = Box<dyn HttpForwardContext + Send>;

#[derive(Clone)]
pub(crate) struct HttpAliveReuseNotes {
    pub keep_alive_leftover: KeepAliveValue,
    pub escaper: ArcEscaper,
}

impl HttpAliveReuseNotes {
    pub(crate) fn from_new(escaper: ArcEscaper) -> Self {
        HttpAliveReuseNotes {
            keep_alive_leftover: KeepAliveValue::default(),
            escaper,
        }
    }

    pub(crate) fn from_alive(escaper: ArcEscaper, keep_alive_leftover: KeepAliveValue) -> Self {
        HttpAliveReuseNotes {
            keep_alive_leftover,
            escaper,
        }
    }

    pub(crate) fn is_exhausted(&self) -> bool {
        self.keep_alive_leftover.is_exhausted()
    }

    /// Overlay this response's Keep-Alive; unset timeout/max stay from leftover.
    pub(crate) fn overlay_keep_alive(&mut self, keep_alive: KeepAliveValue) {
        self.keep_alive_leftover = keep_alive.or_from(self.keep_alive_leftover);
    }
}

#[async_trait]
pub(crate) trait HttpForwardContext {
    async fn check_in_final_escaper(
        &mut self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        is_tls: bool,
    ) -> HttpForwardCapability;

    async fn get_alive_connection(
        &mut self,
        idle_expire: Duration,
    ) -> Option<(BoxHttpForwardConnection, HttpAliveReuseNotes)>;
    async fn make_new_http_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError>;
    async fn make_new_https_connection(
        &mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError>;
    fn save_alive_connection(
        &mut self,
        connection: BoxHttpForwardConnection,
        keep_alive: KeepAliveValue,
        idle_expire: Duration,
    );
    fn fetch_egress_notes(&self, egress_notes: &mut EgressNotes);

    async fn get_prepared_alive_connection(
        &mut self,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        idle_expire: Duration,
        is_tls: bool,
    ) -> Option<(BoxHttpForwardConnection, HttpAliveReuseNotes)> {
        let (connection, reuse_notes) = self.get_alive_connection(idle_expire).await?;
        Some((
            reuse_notes
                .escaper
                .prepare_reused_http_forward_connection(connection, task_notes, task_stats, is_tls),
            reuse_notes,
        ))
    }

    async fn new_prepared_http_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, HttpAliveReuseNotes), TcpConnectError> {
        let (conn, escaper) = self
            .make_new_http_connection(task_conf, task_notes, task_stats, audit_ctx)
            .await?;

        if let Some(escaper_stats) = escaper.get_escape_stats() {
            escaper_stats.add_http_forward_request_attempted();
        }

        Ok((conn, HttpAliveReuseNotes::from_new(escaper)))
    }

    async fn new_prepared_https_connection(
        &mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, HttpAliveReuseNotes), TcpConnectError> {
        let (conn, escaper) = self
            .make_new_https_connection(task_conf, task_notes, task_stats, audit_ctx)
            .await?;

        if let Some(escaper_stats) = escaper.get_escape_stats() {
            escaper_stats.add_https_forward_request_attempted();
        }

        Ok((conn, HttpAliveReuseNotes::from_new(escaper)))
    }
}

struct HttpAliveConnection {
    poller: HttpConnectionEofPoller,
    last_used: Instant,
}

impl HttpAliveConnection {
    fn is_closed(&self) -> bool {
        self.poller.is_closed()
    }
}

#[derive(Default)]
struct HttpAliveReuseState {
    last: Option<HttpAliveConnection>,
    keep_alive_leftover: KeepAliveValue,
    expire_at: Option<Instant>,
}

impl HttpAliveReuseState {
    fn drop_saved(&mut self) {
        self.last = None;
        self.clear_keep_alive_leftover();
    }

    fn take_last(&mut self) -> Option<HttpAliveConnection> {
        self.last.take()
    }

    fn restore_last(&mut self, conn: HttpAliveConnection) {
        self.last = Some(conn);
    }

    fn clear_keep_alive_leftover(&mut self) {
        self.keep_alive_leftover = KeepAliveValue::default();
        self.expire_at = None;
    }

    fn overlay_keep_alive(&mut self, keep_alive: KeepAliveValue) {
        self.keep_alive_leftover = keep_alive.or_from(self.keep_alive_leftover);
    }

    fn cap_expire(&mut self, expire: Option<Instant>) {
        self.expire_at = match (self.expire_at, expire) {
            (Some(x), Some(y)) => Some(x.min(y)),
            (a, b) => a.or(b),
        };
    }

    fn is_expired(&self) -> bool {
        self.expire_at
            .is_some_and(|d| d.saturating_duration_since(Instant::now()).is_zero())
    }

    async fn get_alive(
        &mut self,
        idle_expire: Duration,
    ) -> Option<(BoxHttpForwardConnection, KeepAliveValue)> {
        let conn = match self.last.take() {
            Some(conn) => conn,
            None => {
                self.clear_keep_alive_leftover();
                return None;
            }
        };
        if self.keep_alive_leftover.is_exhausted()
            || conn.last_used.elapsed() >= idle_expire
            || self.is_expired()
        {
            self.clear_keep_alive_leftover();
            return None;
        }
        self.keep_alive_leftover.decrement_max_mut();
        let keep_alive_leftover = self.keep_alive_leftover;
        let connection = conn.poller.recv_conn().await?;
        Some((connection, keep_alive_leftover))
    }

    fn save(
        &mut self,
        connection: BoxHttpForwardConnection,
        keep_alive: KeepAliveValue,
        expire: Option<Instant>,
        idle_expire: Duration,
    ) {
        self.overlay_keep_alive(keep_alive);
        self.cap_expire(expire);
        if self.keep_alive_leftover.is_exhausted() || idle_expire.is_zero() || self.is_expired() {
            return;
        }
        self.last = Some(HttpAliveConnection {
            poller: HttpConnectionEofPoller::spawn(connection),
            last_used: Instant::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ka(s: &[u8]) -> KeepAliveValue {
        let mut v = KeepAliveValue::default();
        v.parse(s);
        v
    }

    #[test]
    fn timeout_is_min_of_header_and_idle_expire() {
        let idle = Duration::from_secs(30);
        assert_eq!(
            parse_ka(b"timeout=5")
                .timeout()
                .map(|t| t.min(idle))
                .unwrap_or(idle),
            Duration::from_secs(5)
        );
        assert_eq!(
            parse_ka(b"timeout=60")
                .timeout()
                .map(|t| t.min(idle))
                .unwrap_or(idle),
            idle
        );
        assert_eq!(
            KeepAliveValue::default()
                .timeout()
                .map(|t| t.min(idle))
                .unwrap_or(idle),
            idle
        );
    }

    #[test]
    fn max_zero_is_not_saved() {
        assert_eq!(
            parse_ka(b"max=0").or_from(KeepAliveValue::default()).max(),
            Some(0)
        );
        assert_eq!(
            KeepAliveValue::default().or_from(parse_ka(b"max=0")).max(),
            Some(0)
        );
    }

    #[test]
    fn new_max_overrides_decremented_leftover() {
        assert_eq!(
            parse_ka(b"max=10").or_from(parse_ka(b"max=3")).max(),
            Some(10)
        );
        assert_eq!(
            KeepAliveValue::default().or_from(parse_ka(b"max=3")).max(),
            Some(3)
        );
    }
}
