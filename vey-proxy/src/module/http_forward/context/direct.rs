/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;

use vey_types::net::{HttpForwardCapability, UpstreamAddr};

use super::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpConnectionEofPoller,
    HttpForwardContext,
};
use crate::audit::AuditContext;
use crate::escape::ArcEscaper;
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectTaskConf, TcpConnectTaskNotes, TlsConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

pub(crate) struct DirectHttpForwardContext {
    escaper: ArcEscaper,
    tcp_notes: TcpConnectTaskNotes,
    last_upstream: UpstreamAddr,
    last_is_tls: bool,
    last_connection: Option<(Instant, HttpConnectionEofPoller)>,
}

impl DirectHttpForwardContext {
    pub(crate) fn new(escaper: ArcEscaper) -> Self {
        DirectHttpForwardContext {
            escaper,
            tcp_notes: TcpConnectTaskNotes::default(),
            last_upstream: UpstreamAddr::empty(),
            last_is_tls: false,
            last_connection: None,
        }
    }
}

#[async_trait]
impl HttpForwardContext for DirectHttpForwardContext {
    async fn check_in_final_escaper(
        &mut self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        is_tls: bool,
    ) -> HttpForwardCapability {
        if self.last_upstream.ne(upstream) || self.last_is_tls != is_tls {
            // new upstream
            self.last_upstream.clone_from(upstream);
            self.tcp_notes.reset();
            // always use different connection for different upstream
            let _old_connection = self.last_connection.take();
        } else {
            // old upstream
        }

        self.escaper._update_egress_path(task_notes);
        self.escaper._local_http_forward_capability()
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
                .map(|c| (c, self.escaper.clone()))
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
        self.escaper._update_audit_context(audit_ctx);
        let conn = self
            .escaper
            ._new_http_forward_connection(task_conf, &mut self.tcp_notes, task_notes, task_stats)
            .await?;
        Ok((conn, self.escaper.clone()))
    }

    async fn make_new_https_connection(
        &mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError> {
        self.last_is_tls = true;
        self.escaper._update_audit_context(audit_ctx);
        let conn = self
            .escaper
            ._new_https_forward_connection(task_conf, &mut self.tcp_notes, task_notes, task_stats)
            .await?;
        Ok((conn, self.escaper.clone()))
    }

    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection) {
        let eof_poller = HttpConnectionEofPoller::spawn(c);
        self.last_connection = Some((Instant::now(), eof_poller));
    }

    fn fetch_tcp_notes(&self, tcp_notes: &mut TcpConnectTaskNotes) {
        tcp_notes.clone_from(&self.tcp_notes);
    }
}
