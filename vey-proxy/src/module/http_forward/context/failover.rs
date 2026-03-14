/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS Developers.
 */

use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::time::Instant;

use vey_types::net::{HttpForwardCapability, UpstreamAddr};

use super::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpConnectionEofPoller,
    HttpForwardContext,
};
use crate::audit::AuditContext;
use crate::escape::{ArcEscaper, RouteEscaperStats};
use crate::module::http_forward::BoxHttpForwardContext;
use crate::module::tcp_connect::{
    TcpConnectError, TcpConnectTaskConf, TcpConnectTaskNotes, TlsConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

struct HttpConnectFailoverContext {
    fwd_ctx: BoxHttpForwardContext,
    audit_ctx: AuditContext,
}

impl HttpConnectFailoverContext {
    fn new(fwd_ctx: BoxHttpForwardContext, audit_ctx: AuditContext) -> Self {
        HttpConnectFailoverContext { fwd_ctx, audit_ctx }
    }

    async fn run_http(
        mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<(Self, BoxHttpForwardConnection, ArcEscaper), (Self, TcpConnectError)> {
        match self
            .fwd_ctx
            .make_new_http_connection(task_conf, task_notes, task_stats, &mut self.audit_ctx)
            .await
        {
            Ok((conn, escaper)) => Ok((self, conn, escaper)),
            Err(e) => Err((self, e)),
        }
    }

    async fn run_https(
        mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<(Self, BoxHttpForwardConnection, ArcEscaper), (Self, TcpConnectError)> {
        match self
            .fwd_ctx
            .make_new_https_connection(task_conf, task_notes, task_stats, &mut self.audit_ctx)
            .await
        {
            Ok((conn, escaper)) => Ok((self, conn, escaper)),
            Err(e) => Err((self, e)),
        }
    }
}

pub(crate) struct FailoverHttpForwardContext {
    route_stats: Arc<RouteEscaperStats>,
    fallback_delay: Duration,
    primary_escaper: ArcEscaper,
    standby_escaper: ArcEscaper,
    primary_forward_ctx: Option<BoxHttpForwardContext>,
    standby_forward_ctx: Option<BoxHttpForwardContext>,
    final_escaper: ArcEscaper,
    tcp_notes: TcpConnectTaskNotes,
    last_upstream: UpstreamAddr,
    last_is_tls: bool,
    last_connection: Option<(Instant, HttpConnectionEofPoller)>,
}

impl FailoverHttpForwardContext {
    pub(crate) fn new(
        primary_escaper: &ArcEscaper,
        standby_escaper: &ArcEscaper,
        fallback_delay: Duration,
        route_stats: Arc<RouteEscaperStats>,
    ) -> Self {
        FailoverHttpForwardContext {
            route_stats,
            fallback_delay,
            primary_escaper: Arc::clone(primary_escaper),
            standby_escaper: Arc::clone(standby_escaper),
            primary_forward_ctx: None,
            standby_forward_ctx: None,
            final_escaper: Arc::clone(primary_escaper),
            tcp_notes: TcpConnectTaskNotes::default(),
            last_upstream: UpstreamAddr::empty(),
            last_is_tls: false,
            last_connection: None,
        }
    }
}

#[async_trait]
impl HttpForwardContext for FailoverHttpForwardContext {
    async fn check_in_final_escaper(
        &mut self,
        task_notes: &ServerTaskNotes,
        upstream: &UpstreamAddr,
        is_tls: bool,
    ) -> HttpForwardCapability {
        if let Some(saved_connection) = self.last_connection.take() {
            if self.last_is_tls == is_tls && self.last_upstream.eq(upstream) {
                self.last_connection = Some(saved_connection);
                // do not set forward context here, we always try all escapers when connect
            } else {
                self.last_upstream.clone_from(upstream);
                self.tcp_notes.reset();
            }
        } else {
            self.last_upstream.clone_from(upstream);
            self.tcp_notes.reset();
        }

        let mut primary_fwd_ctx = self
            .primary_escaper
            .new_http_forward_context(self.primary_escaper.clone());
        let primary_capability = primary_fwd_ctx
            .check_in_final_escaper(task_notes, upstream, is_tls)
            .await;
        self.primary_forward_ctx = Some(primary_fwd_ctx);

        let mut standby_fwd_ctx = self
            .standby_escaper
            .new_http_forward_context(self.standby_escaper.clone());
        let standby_capability = standby_fwd_ctx
            .check_in_final_escaper(task_notes, upstream, is_tls)
            .await;
        self.standby_forward_ctx = Some(standby_fwd_ctx);

        // always return the smallest capability
        primary_capability & standby_capability
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

        let Some(primary_fwd_ctx) = self.primary_forward_ctx.take() else {
            return Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "no next primary escaper available"
            )));
        };
        let Some(standby_fwd_ctx) = self.standby_forward_ctx.take() else {
            return Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "no next standby escaper available"
            )));
        };

        let primary_context = HttpConnectFailoverContext::new(primary_fwd_ctx, audit_ctx.clone());
        let mut primary_task =
            pin!(primary_context.run_http(task_conf, task_notes, task_stats.clone()));

        let standby_context = HttpConnectFailoverContext::new(standby_fwd_ctx, audit_ctx.clone());
        let standby_task = pin!(standby_context.run_http(task_conf, task_notes, task_stats));

        match tokio::time::timeout(self.fallback_delay, &mut primary_task).await {
            Ok(Ok((ctx, conn, escaper))) => {
                *audit_ctx = ctx.audit_ctx;
                ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                self.route_stats.add_request_passed();
                self.final_escaper = escaper.clone();
                return Ok((conn, escaper));
            }
            Ok(Err((_ctx, _e))) => {
                return match standby_task.await {
                    Ok((ctx, conn, escaper)) => {
                        *audit_ctx = ctx.audit_ctx;
                        ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                        self.route_stats.add_request_passed();
                        self.final_escaper = escaper.clone();
                        Ok((conn, escaper))
                    }
                    Err((ctx, e)) => {
                        ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                        self.route_stats.add_request_failed();
                        Err(e)
                    }
                };
            }
            Err(_) => {}
        }

        match futures_util::future::select_ok([primary_task, standby_task]).await {
            Ok(((ctx, conn, escaper), _left)) => {
                *audit_ctx = ctx.audit_ctx;
                ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                self.route_stats.add_request_passed();
                self.final_escaper = escaper.clone();
                Ok((conn, escaper))
            }
            Err((ctx, e)) => {
                ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                self.route_stats.add_request_failed();
                Err(e)
            }
        }
    }

    async fn make_new_https_connection(
        &mut self,
        task_conf: &TlsConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> Result<(BoxHttpForwardConnection, ArcEscaper), TcpConnectError> {
        self.last_is_tls = true;

        let Some(primary_fwd_ctx) = self.primary_forward_ctx.take() else {
            return Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "no next primary escaper available"
            )));
        };
        let Some(standby_fwd_ctx) = self.standby_forward_ctx.take() else {
            return Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "no next standby escaper available"
            )));
        };

        let primary_context = HttpConnectFailoverContext::new(primary_fwd_ctx, audit_ctx.clone());
        let mut primary_task =
            pin!(primary_context.run_https(task_conf, task_notes, task_stats.clone()));

        let standby_context = HttpConnectFailoverContext::new(standby_fwd_ctx, audit_ctx.clone());
        let standby_task = pin!(standby_context.run_https(task_conf, task_notes, task_stats));

        match tokio::time::timeout(self.fallback_delay, &mut primary_task).await {
            Ok(Ok((ctx, conn, escaper))) => {
                *audit_ctx = ctx.audit_ctx;
                ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                self.route_stats.add_request_passed();
                self.final_escaper = escaper.clone();
                return Ok((conn, escaper));
            }
            Ok(Err((_ctx, _e))) => {
                return match standby_task.await {
                    Ok((ctx, conn, escaper)) => {
                        *audit_ctx = ctx.audit_ctx;
                        ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                        self.route_stats.add_request_passed();
                        self.final_escaper = escaper.clone();
                        Ok((conn, escaper))
                    }
                    Err((ctx, e)) => {
                        ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                        self.route_stats.add_request_failed();
                        Err(e)
                    }
                };
            }
            Err(_) => {}
        }

        match futures_util::future::select_ok([primary_task, standby_task]).await {
            Ok(((ctx, conn, escaper), _left)) => {
                *audit_ctx = ctx.audit_ctx;
                ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                self.route_stats.add_request_passed();
                self.final_escaper = escaper.clone();
                Ok((conn, escaper))
            }
            Err((ctx, e)) => {
                ctx.fwd_ctx.fetch_tcp_notes(&mut self.tcp_notes);
                self.route_stats.add_request_failed();
                Err(e)
            }
        }
    }

    fn save_alive_connection(&mut self, c: BoxHttpForwardConnection) {
        let eof_poller = HttpConnectionEofPoller::spawn(c);
        self.last_connection = Some((Instant::now(), eof_poller));
    }

    fn fetch_tcp_notes(&self, tcp_notes: &mut TcpConnectTaskNotes) {
        tcp_notes.clone_from(&self.tcp_notes);
    }
}
