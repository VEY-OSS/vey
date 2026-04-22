/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

use anyhow::anyhow;
use async_trait::async_trait;

use vey_types::net::{HttpForwardCapability, UpstreamAddr};

use super::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpConnectionEofPoller,
    HttpForwardContext,
};
use crate::audit::AuditContext;
use crate::escape::ArcEscaper;
use crate::module::http_forward::BoxHttpForwardContext;
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectTaskConf, TcpConnectTaskNotes, TlsConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

pub(crate) struct RouteHttpForwardContext {
    escaper: ArcEscaper,
    fwd_ctx: Option<BoxHttpForwardContext>,
    run_local_update: bool,
    final_escaper: ArcEscaper,
    tcp_notes: TcpConnectTaskNotes,
    last_upstream: UpstreamAddr,
    last_is_tls: bool,
    last_connection: Option<(Instant, HttpConnectionEofPoller)>,
}

impl RouteHttpForwardContext {
    pub(crate) fn new(escaper: ArcEscaper) -> Self {
        let fake_final_escaper = Arc::clone(&escaper);
        RouteHttpForwardContext {
            escaper,
            fwd_ctx: None,
            run_local_update: false,
            final_escaper: fake_final_escaper,
            tcp_notes: TcpConnectTaskNotes::default(),
            last_upstream: UpstreamAddr::empty(),
            last_is_tls: false,
            last_connection: None,
        }
    }
}

#[async_trait]
impl HttpForwardContext for RouteHttpForwardContext {
    async fn check_in_final_escaper(
        &mut self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        is_tls: bool,
    ) -> HttpForwardCapability {
        if let Some((started, eof_poller)) = self.last_connection.take() {
            if self.last_is_tls == is_tls
                && self.last_upstream.eq(upstream)
                && !eof_poller.is_closed()
            {
                let mut fwd_ctx = self
                    .final_escaper
                    .new_http_forward_context(self.final_escaper.clone());
                let capability = fwd_ctx
                    .check_in_final_escaper(task_notes, upstream, is_tls)
                    .await;
                // when resue saved alive connection, make sure we reconnect on the same escaper
                self.fwd_ctx = Some(fwd_ctx);
                self.run_local_update = false;
                self.last_connection = Some((started, eof_poller));
                return capability;
            } else {
                drop(eof_poller);
            }
        }

        self.escaper._update_egress_path(task_notes);
        self.run_local_update = true;
        if let Some(next_escaper) = self
            .escaper
            ._check_out_next_escaper(task_notes, upstream)
            .await
        {
            let mut fwd_ctx = next_escaper.new_http_forward_context(next_escaper.clone());
            let capability = fwd_ctx
                .check_in_final_escaper(task_notes, upstream, is_tls)
                .await;
            self.fwd_ctx = Some(fwd_ctx);
            capability
        } else if !Arc::ptr_eq(&self.escaper, &self.final_escaper) {
            let mut fwd_ctx = self
                .final_escaper
                .new_http_forward_context(self.final_escaper.clone());
            let capability = fwd_ctx
                .check_in_final_escaper(task_notes, upstream, is_tls)
                .await;
            self.fwd_ctx = Some(fwd_ctx);
            capability
        } else {
            self.fwd_ctx = None;
            HttpForwardCapability::default()
        }
    }

    async fn get_alive_connection(
        &mut self,
        idle_expire: Duration,
    ) -> Option<(BoxHttpForwardConnection, ArcEscaper)> {
        let (instant, eof_poller) = self.last_connection.take()?;
        if instant.elapsed() < idle_expire {
            eof_poller
                .recv_conn()
                .await
                .map(|c| (c, self.final_escaper.clone()))
        } else {
            None
        }
    }

    async fn make_new_http_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError> {
        self.last_is_tls = false;
        if self.run_local_update {
            self.escaper._update_audit_context(audit_ctx);
        }
        let Some(mut fwd_ctx) = self.fwd_ctx.take() else {
            return Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "no next escaper selected"
            )));
        };
        let (conn, escaper) = fwd_ctx
            .make_new_http_connection(task_conf, task_notes, task_stats, audit_ctx)
            .await?;
        self.final_escaper = escaper.clone();
        Ok((conn, escaper))
    }

    async fn make_new_https_connection(
        &mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError> {
        self.last_is_tls = true;
        if self.run_local_update {
            self.escaper._update_audit_context(audit_ctx);
        }
        let Some(mut fwd_ctx) = self.fwd_ctx.take() else {
            return Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "no next escaper selected"
            )));
        };
        let (conn, escaper) = fwd_ctx
            .make_new_https_connection(task_conf, task_notes, task_stats, audit_ctx)
            .await?;
        self.final_escaper = escaper.clone();
        Ok((conn, escaper))
    }

    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection) {
        let eof_poller = HttpConnectionEofPoller::spawn(c);
        self.last_connection = Some((Instant::now(), eof_poller));
    }

    fn fetch_tcp_notes(&self, tcp_notes: &mut TcpConnectTaskNotes) {
        tcp_notes.clone_from(&self.tcp_notes);
    }
}
