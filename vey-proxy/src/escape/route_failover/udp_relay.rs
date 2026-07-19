/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::pin::pin;

use anyhow::anyhow;

use super::RouteFailoverEscaper;
use crate::escape::{ArcEscaper, EgressNotes};
use crate::module::udp_connect::UdpConnectError;
use crate::module::udp_relay::{ArcUdpRelayTaskRemoteStats, UdpRelaySetupResult, UdpRelayTaskConf};
use crate::serve::ServerTaskNotes;

struct UdpRelayFailoverContext {
    egress_notes: EgressNotes,
    setup_result: UdpRelaySetupResult,
}

impl UdpRelayFailoverContext {
    fn new() -> Self {
        UdpRelayFailoverContext {
            egress_notes: EgressNotes::default(),
            setup_result: Err(UdpConnectError::EscaperNotUsable(anyhow!(
                "no udp set relay called yet"
            ))),
        }
    }

    async fn run(
        mut self,
        escaper: &ArcEscaper,
        task_conf: &UdpRelayTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> Self {
        self.setup_result = escaper
            .udp_setup_relay(task_conf, &mut self.egress_notes, task_notes, task_stats)
            .await;
        self
    }
}

impl RouteFailoverEscaper {
    pub(super) async fn udp_setup_relay_with_failover(
        &self,
        task_conf: &UdpRelayTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> UdpRelaySetupResult {
        let primary_context = UdpRelayFailoverContext::new();
        let mut primary_task = pin!(primary_context.run(
            &self.primary_node,
            task_conf,
            task_notes,
            task_stats.clone()
        ));

        if let Ok(ctx) = tokio::time::timeout(self.config.fallback_delay, &mut primary_task).await {
            return match ctx.setup_result {
                Ok(c) => {
                    self.stats.add_request_passed();
                    *egress_notes = ctx.egress_notes;
                    Ok(c)
                }
                Err(_e) => {
                    match self
                        .standby_node
                        .udp_setup_relay(task_conf, egress_notes, task_notes, task_stats)
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

        let standby_context = UdpRelayFailoverContext::new();
        let standby_task =
            pin!(standby_context.run(&self.standby_node, task_conf, task_notes, task_stats));

        let (ctx, left) = futures_util::future::select(primary_task, standby_task)
            .await
            .into_inner();
        match ctx.setup_result {
            Ok(c) => {
                self.stats.add_request_passed();
                *egress_notes = ctx.egress_notes;
                Ok(c)
            }
            Err(_e) => {
                let ctx = left.await;
                match ctx.setup_result {
                    Ok(c) => {
                        self.stats.add_request_passed();
                        *egress_notes = ctx.egress_notes;
                        Ok(c)
                    }
                    Err(e) => {
                        self.stats.add_request_failed();
                        *egress_notes = ctx.egress_notes;
                        Err(e)
                    }
                }
            }
        }
    }
}
