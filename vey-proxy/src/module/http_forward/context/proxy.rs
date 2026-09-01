/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use async_trait::async_trait;

use vey_http::header::KeepAliveValue;
use vey_types::net::{HttpForwardCapability, UpstreamAddr};

use super::HttpAliveReuseState;
use crate::audit::AuditContext;
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpForwardContext,
};
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;

pub(crate) struct ProxyHttpForwardContext {
    escaper: ArcEscaper,
    egress_notes: EgressNotes,
    last_upstream: UpstreamAddr,
    last_is_tls: bool,
    reuse: HttpAliveReuseState,
}

impl ProxyHttpForwardContext {
    pub(crate) fn new(escaper: ArcEscaper) -> Self {
        ProxyHttpForwardContext {
            escaper,
            egress_notes: EgressNotes::default(),
            last_upstream: UpstreamAddr::empty(),
            last_is_tls: false,
            reuse: HttpAliveReuseState::default(),
        }
    }
}

#[async_trait]
impl HttpForwardContext for ProxyHttpForwardContext {
    async fn check_in_final_escaper(
        &mut self,
        _task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        is_tls: bool,
    ) -> HttpForwardCapability {
        if is_tls {
            if !self.last_is_tls || self.last_upstream.ne(upstream) {
                // new upstream, but not new peer
                self.last_upstream.clone_from(upstream);
                self.egress_notes.reset();
                // use new tls session
                self.reuse.drop_saved();
            } else {
                // old upstream and reuse tls session
            }
        } else if self.last_is_tls {
            // new upstream, but not new peer
            self.last_upstream.clone_from(upstream);
            self.egress_notes.reset();
            // drop old tls session
            self.reuse.drop_saved();
        } else if self.last_upstream.ne(upstream) {
            // new upstream, but not new peer
            self.last_upstream.clone_from(upstream);
        } else {
            // old upstream
        }

        self.escaper._update_egress_path(_task_notes);
        self.escaper._local_http_forward_capability()
    }

    async fn get_alive_connection(
        &mut self,
        idle_expire: Duration,
    ) -> Option<(BoxHttpForwardConnection, ArcEscaper)> {
        self.reuse
            .get_alive(idle_expire)
            .await
            .map(|c| (c, self.escaper.clone()))
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
        self.escaper._update_audit_context(audit_ctx);
        let conn = self
            .escaper
            ._new_http_forward_connection(task_conf, &mut self.egress_notes, task_notes, task_stats)
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
        self.reuse.clear_inflight();
        self.escaper._update_audit_context(audit_ctx);
        let conn = self
            .escaper
            ._new_https_forward_connection(
                task_conf,
                &mut self.egress_notes,
                task_notes,
                task_stats,
            )
            .await?;
        Ok((conn, self.escaper.clone()))
    }

    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection, ka: KeepAliveValue) {
        self.reuse.save(c, ka);
    }

    fn fetch_egress_notes(&self, egress_notes: &mut EgressNotes) {
        // the upstream addr self.notes is the proxy_addr,
        // which is likely to be different from the one in egress_notes
        egress_notes.clone_from(&self.egress_notes);
    }
}
