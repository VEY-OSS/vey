/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;
use vey_io_ext::{LimitedUdpCopyRemoteRecv, LimitedUdpCopyRemoteSend};

use super::StackSocks5Escaper;
use crate::escape::EgressNotes;
use crate::module::udp_connect::{
    UdpConnectRemoteWrapperStats, UdpConnectResult, UdpConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

mod recv;
mod send;

pub(crate) use recv::StackSocks5UdpConnectRemoteRecv;
pub(crate) use send::StackSocks5UdpConnectRemoteSend;

impl StackSocks5Escaper {
    pub(super) async fn udp_new_connection(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        let (ctl_stream, udp_c) = self
            .timed_socks5_udp_associate(egress_notes, task_notes)
            .await?;

        let mut wrapper_stats = UdpConnectRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let recv = LimitedUdpCopyRemoteRecv::local_limited(
            udp_c.0,
            self.config.general.udp_sock_speed_limit.shift_millis,
            self.config.general.udp_sock_speed_limit.max_south_packets,
            self.config.general.udp_sock_speed_limit.max_south_bytes,
            wrapper_stats.clone(),
        );
        let send = LimitedUdpCopyRemoteSend::local_limited(
            udp_c.1,
            self.config.general.udp_sock_speed_limit.shift_millis,
            self.config.general.udp_sock_speed_limit.max_north_packets,
            self.config.general.udp_sock_speed_limit.max_north_bytes,
            wrapper_stats,
        );

        let recv = StackSocks5UdpConnectRemoteRecv::new(
            recv,
            ctl_stream,
            self.config.end_on_control_closed,
            self.escape_logger.clone(),
        );
        let send = StackSocks5UdpConnectRemoteSend::new(
            send,
            task_conf.upstream,
            self.escape_logger.clone(),
        );

        Ok((Box::new(recv), Box::new(send)))
    }

    pub(super) async fn nested_udp_connect(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> UdpConnectResult {
        let (ctl_stream, udp_c) = self
            .timed_socks5_udp_associate(egress_notes, task_notes)
            .await?;

        let recv = LimitedUdpCopyRemoteRecv::local_limited(
            udp_c.0,
            self.config.general.udp_sock_speed_limit.shift_millis,
            self.config.general.udp_sock_speed_limit.max_south_packets,
            self.config.general.udp_sock_speed_limit.max_south_bytes,
            self.stats.clone(),
        );
        let send = LimitedUdpCopyRemoteSend::local_limited(
            udp_c.1,
            self.config.general.udp_sock_speed_limit.shift_millis,
            self.config.general.udp_sock_speed_limit.max_north_packets,
            self.config.general.udp_sock_speed_limit.max_north_bytes,
            self.stats.clone(),
        );

        let recv = StackSocks5UdpConnectRemoteRecv::new(
            recv,
            ctl_stream,
            self.config.end_on_control_closed,
            self.escape_logger.clone(),
        );
        let send = StackSocks5UdpConnectRemoteSend::new(
            send,
            task_conf.upstream,
            self.escape_logger.clone(),
        );

        Ok((Box::new(recv), Box::new(send)))
    }
}
