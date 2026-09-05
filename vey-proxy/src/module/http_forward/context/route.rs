/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;

use vey_types::net::{HttpForwardCapability, KeepAliveValue, UpstreamAddr};

use super::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpAliveReuseNotes,
    HttpAliveReuseState, HttpForwardContext,
};
use crate::audit::AuditContext;
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::http_forward::BoxHttpForwardContext;
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;

pub(crate) struct RouteHttpForwardContext {
    escaper: ArcEscaper,
    fwd_ctx: Option<BoxHttpForwardContext>,
    run_local_update: bool,
    final_escaper: ArcEscaper,
    egress_notes: EgressNotes,
    last_upstream: UpstreamAddr,
    last_is_tls: bool,
    reuse: HttpAliveReuseState,
}

impl RouteHttpForwardContext {
    pub(crate) fn new(escaper: ArcEscaper) -> Self {
        let fake_final_escaper = Arc::clone(&escaper);
        RouteHttpForwardContext {
            escaper,
            fwd_ctx: None,
            run_local_update: false,
            final_escaper: fake_final_escaper,
            egress_notes: EgressNotes::default(),
            last_upstream: UpstreamAddr::empty(),
            last_is_tls: false,
            reuse: HttpAliveReuseState::default(),
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
        if let Some(saved) = self.reuse.take_last() {
            if self.last_is_tls == is_tls && self.last_upstream.eq(upstream) && !saved.is_closed() {
                let mut fwd_ctx = self
                    .final_escaper
                    .new_http_forward_context(self.final_escaper.clone());
                let capability = fwd_ctx
                    .check_in_final_escaper(task_notes, upstream, is_tls)
                    .await;
                // when resue saved alive connection, make sure we reconnect on the same escaper
                self.fwd_ctx = Some(fwd_ctx);
                self.run_local_update = false;
                self.reuse.restore_last(saved);
                return capability;
            } else {
                drop(saved);
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
    ) -> Option<(BoxHttpForwardConnection, HttpAliveReuseNotes)> {
        self.reuse
            .get_alive(idle_expire)
            .await
            .map(|(c, leftover)| {
                (
                    c,
                    HttpAliveReuseNotes {
                        leftover,
                        escaper: self.final_escaper.clone(),
                    },
                )
            })
    }

    async fn make_new_http_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError> {
        self.last_is_tls = false;
        self.reuse.clear_inflight();
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
        fwd_ctx.fetch_egress_notes(&mut self.egress_notes);
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
        self.reuse.clear_inflight();
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
        fwd_ctx.fetch_egress_notes(&mut self.egress_notes);
        self.final_escaper = escaper.clone();
        Ok((conn, escaper))
    }

    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection, ka: KeepAliveValue) {
        self.reuse.save(c, ka);
    }

    fn fetch_egress_notes(&self, egress_notes: &mut EgressNotes) {
        egress_notes.clone_from(&self.egress_notes);
    }
}
