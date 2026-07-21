/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::pin;

use anyhow::anyhow;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;

use super::RouteFailoverEscaper;
use crate::audit::AuditContext;
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::tcp_connect::{TcpConnectError, TcpConnectResult, TcpConnectTaskConf};
use crate::serve::ServerTaskNotes;

pub struct TcpConnectFailoverContext {
    egress_notes: EgressNotes,
    audit_ctx: AuditContext,
    connect_result: TcpConnectResult,
}

impl TcpConnectFailoverContext {
    fn new(audit_ctx: &AuditContext) -> Self {
        TcpConnectFailoverContext {
            egress_notes: EgressNotes::default(),
            audit_ctx: audit_ctx.clone(),
            connect_result: Err(TcpConnectError::EscaperNotUsable(anyhow!(
                "tcp setup connection not called yet"
            ))),
        }
    }

    async fn run(
        mut self,
        escaper: &ArcEscaper,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> Self {
        self.connect_result = escaper
            .tcp_setup_connection(
                task_conf,
                &mut self.egress_notes,
                task_notes,
                task_stats,
                &mut self.audit_ctx,
            )
            .await;
        self
    }
}

impl RouteFailoverEscaper {
    pub(super) async fn tcp_setup_connection_with_failover(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
        audit_ctx: &mut AuditContext,
    ) -> TcpConnectResult {
        let primary_context = TcpConnectFailoverContext::new(audit_ctx);
        let mut primary_task = pin!(primary_context.run(
            &self.primary_node,
            task_conf,
            task_notes,
            task_stats.clone()
        ));

        if let Ok(ctx) = tokio::time::timeout(self.config.fallback_delay, &mut primary_task).await {
            return match ctx.connect_result {
                Ok(c) => {
                    self.stats.add_request_passed();
                    *audit_ctx = ctx.audit_ctx;
                    *egress_notes = ctx.egress_notes;
                    Ok(c)
                }
                Err(_e) => {
                    match self
                        .standby_node
                        .tcp_setup_connection(
                            task_conf,
                            egress_notes,
                            task_notes,
                            task_stats,
                            audit_ctx,
                        )
                        .await
                    {
                        Ok(c) => {
                            self.stats.add_request_passed();
                            Ok(c)
                        }
                        Err(e) => {
                            self.stats.add_request_failed();
                            Err(e)
                        }
                    }
                }
            };
        }

        let standby_context = TcpConnectFailoverContext::new(audit_ctx);
        let standby_task =
            pin!(standby_context.run(&self.standby_node, task_conf, task_notes, task_stats));

        let (ctx, left) = futures_util::future::select(primary_task, standby_task)
            .await
            .into_inner();
        match ctx.connect_result {
            Ok(c) => {
                self.stats.add_request_passed();
                *audit_ctx = ctx.audit_ctx;
                *egress_notes = ctx.egress_notes;
                Ok(c)
            }
            Err(_e) => {
                let ctx = left.await;
                match ctx.connect_result {
                    Ok(c) => {
                        self.stats.add_request_passed();
                        *audit_ctx = ctx.audit_ctx;
                        *egress_notes = ctx.egress_notes;
                        Ok(c)
                    }
                    Err(e) => {
                        self.stats.add_request_failed();
                        *audit_ctx = ctx.audit_ctx;
                        *egress_notes = ctx.egress_notes;
                        Err(e)
                    }
                }
            }
        }
    }
}
