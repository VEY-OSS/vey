/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;
use vey_io_ext::{LimitedUdpRecv, LimitedUdpSend};

use super::ProxySocks5sEscaper;
use crate::escape::EgressNotes;
use crate::escape::proxy_socks5::udp_connect::{
    ProxySocks5UdpConnectRemoteRecv, ProxySocks5UdpConnectRemoteSend,
};
use crate::module::udp_connect::{
    UdpConnectRemoteWrapperStats, UdpConnectResult, UdpConnectTaskConf, UdpConnectTaskNotes,
};
use crate::serve::ServerTaskNotes;

impl ProxySocks5sEscaper {
    pub(super) async fn udp_connect_to(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        udp_notes: &mut UdpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        let mut egress_notes = EgressNotes::default();
        let (ctl_stream, udp_socket, udp_local_addr, udp_peer_addr) = self
            .timed_socks5_udp_associate(
                self.config.udp_socket_buffer,
                &mut egress_notes,
                task_notes,
            )
            .await?;
        udp_notes.egress = egress_notes.egress;
        udp_notes.override_peer = egress_notes.override_peer;

        udp_notes.local = Some(udp_local_addr);
        udp_notes.next = Some(udp_peer_addr);

        let mut wrapper_stats = UdpConnectRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let (recv, send) = vey_io_ext::split_udp(udp_socket);
        let recv = LimitedUdpRecv::local_limited(
            recv,
            self.config.general.udp_sock_speed_limit.shift_millis,
            self.config.general.udp_sock_speed_limit.max_south_packets,
            self.config.general.udp_sock_speed_limit.max_south_bytes,
            wrapper_stats.clone(),
        );
        let send = LimitedUdpSend::local_limited(
            send,
            self.config.general.udp_sock_speed_limit.shift_millis,
            self.config.general.udp_sock_speed_limit.max_north_packets,
            self.config.general.udp_sock_speed_limit.max_north_bytes,
            wrapper_stats,
        );

        let recv = ProxySocks5UdpConnectRemoteRecv::new(
            recv,
            ctl_stream,
            self.config.end_on_control_closed,
            self.escape_logger.clone(),
        );
        let send = ProxySocks5UdpConnectRemoteSend::new(
            send,
            task_conf.upstream,
            self.escape_logger.clone(),
        );

        Ok((Box::new(recv), Box::new(send)))
    }
}
