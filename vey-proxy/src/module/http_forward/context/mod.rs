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
    pub leftover: KeepAliveValue,
    pub escaper: ArcEscaper,
}

impl HttpAliveReuseNotes {
    pub(crate) fn from_new(escaper: ArcEscaper) -> Self {
        HttpAliveReuseNotes {
            leftover: KeepAliveValue::default(),
            escaper,
        }
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
    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection, ka: KeepAliveValue);
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
    saved_at: Instant,
    poller: HttpConnectionEofPoller,
    keep_alive: KeepAliveValue,
}

impl HttpAliveConnection {
    fn is_closed(&self) -> bool {
        self.poller.is_closed()
    }
}

#[derive(Default)]
struct HttpAliveReuseState {
    last: Option<HttpAliveConnection>,
    inflight: Option<KeepAliveValue>,
}

impl HttpAliveReuseState {
    fn drop_saved(&mut self) {
        self.last = None;
        self.inflight = None;
    }

    fn take_last(&mut self) -> Option<HttpAliveConnection> {
        self.last.take()
    }

    fn restore_last(&mut self, conn: HttpAliveConnection) {
        self.last = Some(conn);
    }

    fn clear_inflight(&mut self) {
        self.inflight = None;
    }

    async fn get_alive(
        &mut self,
        idle_expire: Duration,
    ) -> Option<(BoxHttpForwardConnection, KeepAliveValue)> {
        let conn = match self.last.take() {
            Some(conn) => conn,
            None => {
                self.inflight = None;
                return None;
            }
        };
        if conn.keep_alive.max() == Some(0) {
            return None;
        }
        let timeout = conn
            .keep_alive
            .timeout()
            .map(|t| t.min(idle_expire))
            .unwrap_or(idle_expire);
        if conn.saved_at.elapsed() >= timeout {
            return None;
        }
        let leftover = conn.keep_alive.decrement_max();
        let c = conn.poller.recv_conn().await?;
        self.inflight = Some(leftover);
        Some((c, leftover))
    }

    fn save(&mut self, c: BoxHttpForwardConnection, ka: KeepAliveValue) {
        let ka = ka.or_from(self.inflight.take().unwrap_or_default());
        if ka.max() == Some(0) {
            return;
        }
        self.last = Some(HttpAliveConnection {
            saved_at: Instant::now(),
            poller: HttpConnectionEofPoller::spawn(c),
            keep_alive: ka,
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
