/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;
use vey_io_ext::{LimitedUdpRecv, LimitedUdpSend};

use super::{ProxyFloatEscaper, ProxyFloatSocks5sPeer};
use crate::escape::EgressNotes;
use crate::escape::proxy_socks5::udp_connect::{
    ProxySocks5UdpConnectRemoteRecv, ProxySocks5UdpConnectRemoteSend,
};
use crate::module::udp_connect::{
    UdpConnectRemoteWrapperStats, UdpConnectResult, UdpConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

impl ProxyFloatSocks5sPeer {
    pub(super) async fn udp_connect_to(
        &self,
        escaper: &ProxyFloatEscaper,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        let (ctl_stream, udp_socket) = self
            .timed_socks5_udp_associate(
                escaper,
                escaper.config.udp_socket_buffer,
                egress_notes,
                task_notes,
            )
            .await?;

        let mut wrapper_stats =
            UdpConnectRemoteWrapperStats::new(escaper.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(escaper.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let (recv, send) = vey_io_ext::split_udp(udp_socket);
        let recv = LimitedUdpRecv::local_limited(
            recv,
            self.udp_sock_speed_limit.shift_millis,
            self.udp_sock_speed_limit.max_south_packets,
            self.udp_sock_speed_limit.max_south_bytes,
            wrapper_stats.clone(),
        );
        let send = LimitedUdpSend::local_limited(
            send,
            self.udp_sock_speed_limit.shift_millis,
            self.udp_sock_speed_limit.max_north_packets,
            self.udp_sock_speed_limit.max_north_bytes,
            wrapper_stats,
        );

        let recv = ProxySocks5UdpConnectRemoteRecv::new(
            recv,
            ctl_stream,
            self.end_on_control_closed,
            escaper.escape_logger.clone(),
        );
        let send = ProxySocks5UdpConnectRemoteSend::new(
            send,
            task_conf.upstream,
            escaper.escape_logger.clone(),
        );

        Ok((Box::new(recv), Box::new(send)))
    }
}
