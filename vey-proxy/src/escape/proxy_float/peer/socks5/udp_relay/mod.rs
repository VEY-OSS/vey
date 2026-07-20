/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_io_ext::{LimitedUdpRecv, LimitedUdpSend};

use super::{ProxyFloatEscaper, ProxyFloatSocks5Peer};
use crate::escape::EgressNotes;
use crate::escape::proxy_socks5::udp_relay::{
    ProxySocks5UdpRelayRemoteRecv, ProxySocks5UdpRelayRemoteSend,
};
use crate::module::udp_relay::{
    ArcUdpRelayTaskRemoteStats, UdpRelayRemoteWrapperStats, UdpRelaySetupResult, UdpRelayTaskConf,
};
use crate::serve::ServerTaskNotes;

impl ProxyFloatSocks5Peer {
    pub(super) async fn udp_setup_relay(
        &self,
        escaper: &ProxyFloatEscaper,
        task_conf: &UdpRelayTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> UdpRelaySetupResult {
        let (ctl_stream, udp_socket) = self
            .timed_socks5_udp_associate(escaper, task_conf.sock_buf, egress_notes, task_notes)
            .await?;

        let mut wrapper_stats = UdpRelayRemoteWrapperStats::new(escaper.stats.clone(), task_stats);
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

        let udp_local_addr = egress_notes.udp.local.unwrap();
        let udp_peer_addr = egress_notes.udp.peer.unwrap();
        let recv = ProxySocks5UdpRelayRemoteRecv::new(
            recv,
            udp_local_addr,
            udp_peer_addr,
            ctl_stream,
            self.end_on_control_closed,
            escaper.escape_logger.clone(),
        );
        let send = ProxySocks5UdpRelayRemoteSend::new(
            send,
            udp_local_addr,
            udp_peer_addr,
            escaper.escape_logger.clone(),
        );

        Ok((Box::new(recv), Box::new(send)))
    }
}
