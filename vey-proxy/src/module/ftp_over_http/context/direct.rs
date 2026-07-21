/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use async_trait::async_trait;

use vey_types::net::UpstreamAddr;

use super::{
    ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpRemoteConnection,
    FtpConnectContext,
};
use crate::escape::ArcEscaper;
use crate::escape::EgressNotes;
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf};
use crate::serve::ServerTaskNotes;

pub(crate) struct DirectFtpConnectContext {
    escaper: ArcEscaper,
    upstream: UpstreamAddr,
    control_egress_notes: EgressNotes,
    transfer_egress_notes: EgressNotes,
}

impl DirectFtpConnectContext {
    pub(crate) fn new(escaper: ArcEscaper, upstream: UpstreamAddr) -> Self {
        DirectFtpConnectContext {
            escaper,
            upstream,
            control_egress_notes: EgressNotes::default(),
            transfer_egress_notes: EgressNotes::default(),
        }
    }
}

#[async_trait]
impl FtpConnectContext for DirectFtpConnectContext {
    async fn new_control_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        self.escaper
            ._new_ftp_control_connection(
                task_conf,
                &mut self.control_egress_notes,
                task_notes,
                task_stats,
            )
            .await
    }

    fn fetch_control_egress_notes(&self, egress_notes: &mut EgressNotes) {
        egress_notes.clone_from(&self.control_egress_notes);
    }

    async fn new_transfer_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteTransferStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        self.escaper
            ._new_ftp_transfer_connection(
                task_conf,
                &mut self.transfer_egress_notes,
                &self.control_egress_notes,
                task_notes,
                task_stats,
                &self.upstream,
            )
            .await
    }

    fn fetch_transfer_egress_notes(&self, egress_notes: &mut EgressNotes) {
        egress_notes.clone_from(&self.transfer_egress_notes);
    }
}
