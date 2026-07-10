/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::pin;

use anyhow::anyhow;

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;

use super::RouteFailoverEscaper;
use crate::escape::ArcEscaper;
use crate::module::udp_connect::{
    UdpConnectError, UdpConnectResult, UdpConnectTaskConf, UdpConnectTaskNotes,
};
use crate::serve::ServerTaskNotes;

struct UdpConnectFailoverContext {
    udp_notes: UdpConnectTaskNotes,
    connect_result: UdpConnectResult,
}

impl UdpConnectFailoverContext {
    fn new() -> Self {
        UdpConnectFailoverContext {
            udp_notes: UdpConnectTaskNotes::default(),
            connect_result: Err(UdpConnectError::EscaperNotUsable(anyhow!(
                "no udp setup connection called yet"
            ))),
        }
    }

    async fn run(
        mut self,
        escaper: &ArcEscaper,
        task_conf: &UdpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> Self {
        self.connect_result = escaper
            .udp_setup_connection(task_conf, &mut self.udp_notes, task_notes, task_stats)
            .await;
        self
    }
}

impl RouteFailoverEscaper {
    pub(super) async fn udp_setup_connection_with_failover(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        udp_notes: &mut UdpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        let primary_context = UdpConnectFailoverContext::new();
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
                    *udp_notes = ctx.udp_notes;
                    Ok(c)
                }
                Err(_e) => {
                    match self
                        .standby_node
                        .udp_setup_connection(task_conf, udp_notes, task_notes, task_stats)
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

        let standby_context = UdpConnectFailoverContext::new();
        let standby_task =
            pin!(standby_context.run(&self.standby_node, task_conf, task_notes, task_stats));

        let (ctx, left) = futures_util::future::select(primary_task, standby_task)
            .await
            .into_inner();
        match ctx.connect_result {
            Ok(c) => {
                self.stats.add_request_passed();
                *udp_notes = ctx.udp_notes;
                Ok(c)
            }
            Err(_e) => {
                let ctx = left.await;
                match ctx.connect_result {
                    Ok(c) => {
                        self.stats.add_request_passed();
                        *udp_notes = ctx.udp_notes;
                        Ok(c)
                    }
                    Err(e) => {
                        self.stats.add_request_failed();
                        *udp_notes = ctx.udp_notes;
                        Err(e)
                    }
                }
            }
        }
    }
}
