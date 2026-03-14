/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use async_trait::async_trait;

use vey_types::net::{HttpForwardCapability, UpstreamAddr};

use super::{ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpConnectionEofPoller};
use crate::audit::AuditContext;
use crate::escape::ArcEscaper;
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectTaskConf, TcpConnectTaskNotes, TlsConnectTaskConf,
};
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
    ) -> Option<(BoxHttpForwardConnection, ArcEscaper)>;
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
    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection);
    fn fetch_tcp_notes(&self, tcp_notes: &mut TcpConnectTaskNotes);

    async fn get_prepared_alive_connection(
        &mut self,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        idle_expire: Duration,
        is_tls: bool,
    ) -> Option<BoxHttpForwardConnection> {
        let (mut connection, escaper) = self.get_alive_connection(idle_expire).await?;

        let all_user_stats = task_notes
            .user_ctx()
            .map(|ctx| {
                escaper
                    .get_escape_stats()
                    .map(|s| ctx.fetch_upstream_traffic_stats(s.name(), s.share_extra_tags()))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        connection
            .0
            .update_stats(&task_stats, all_user_stats.clone());
        connection.1.update_stats(&task_stats, all_user_stats);

        if let Some(escaper_stats) = escaper.get_escape_stats() {
            if is_tls {
                escaper_stats.add_https_forward_request_attempted();
            } else {
                escaper_stats.add_http_forward_request_attempted();
            }
        }

        Some(connection)
    }

    async fn new_prepared_http_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        let (conn, escaper) = self
            .make_new_http_connection(task_conf, task_notes, task_stats, audit_ctx)
            .await?;

        if let Some(escaper_stats) = escaper.get_escape_stats() {
            escaper_stats.add_http_forward_request_attempted();
        }

        Ok(conn)
    }

    async fn new_prepared_https_connection(
        &mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        let (conn, escaper) = self
            .make_new_https_connection(task_conf, task_notes, task_stats, audit_ctx)
            .await?;

        if let Some(escaper_stats) = escaper.get_escape_stats() {
            escaper_stats.add_https_forward_request_attempted();
        }

        Ok(conn)
    }
}
