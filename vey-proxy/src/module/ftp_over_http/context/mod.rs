/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use async_trait::async_trait;

use super::{ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpRemoteConnection};
use crate::escape::EgressNotes;
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf};
use crate::serve::ServerTaskNotes;

mod deny;
pub(crate) use deny::DenyFtpConnectContext;

mod direct;
pub(crate) use direct::DirectFtpConnectContext;

#[async_trait]
pub(crate) trait FtpConnectContext {
    async fn new_control_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError>;
    fn fetch_control_egress_notes(&self, egress_notes: &mut EgressNotes);

    async fn new_transfer_connection(
        &mut self,
        task_conf: &TcpConnectTaskConf<'_>,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteTransferStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError>;
    fn fetch_transfer_egress_notes(&self, egress_notes: &mut EgressNotes);
}

pub(crate) type BoxFtpConnectContext = Box<dyn FtpConnectContext + Send>;
