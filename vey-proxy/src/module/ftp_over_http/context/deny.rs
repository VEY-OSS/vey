/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use async_trait::async_trait;

use vey_types::metrics::NodeName;

use super::{
    ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpRemoteConnection,
    FtpConnectContext,
};
use crate::escape::EgressNotes;
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf};
use crate::serve::ServerTaskNotes;

pub(crate) struct DenyFtpConnectContext {
    escaper_name: NodeName,
    control_error: Option<TcpConnectError>,
}

impl DenyFtpConnectContext {
    pub(crate) fn new(escaper_name: &NodeName, error: Option<TcpConnectError>) -> Self {
        DenyFtpConnectContext {
            escaper_name: escaper_name.clone(),
            control_error: error,
        }
    }
}

#[async_trait]
impl FtpConnectContext for DenyFtpConnectContext {
    async fn new_control_connection(
        &mut self,
        _task_conf: &TcpConnectTaskConf<'_>,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        if let Some(e) = self.control_error.take() {
            Err(e)
        } else {
            Err(TcpConnectError::MethodUnavailable)
        }
    }

    fn fetch_control_egress_notes(&self, egress_notes: &mut EgressNotes) {
        egress_notes.escaper.clone_from(&self.escaper_name)
    }

    async fn new_transfer_connection(
        &mut self,
        _task_conf: &TcpConnectTaskConf<'_>,
        _task_notes: &ServerTaskNotes,
        _task_stats: ArcFtpTaskRemoteTransferStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        Err(TcpConnectError::MethodUnavailable)
    }

    fn fetch_transfer_egress_notes(&self, egress_notes: &mut EgressNotes) {
        egress_notes.escaper.clone_from(&self.escaper_name)
    }
}
