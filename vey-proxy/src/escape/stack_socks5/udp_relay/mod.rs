/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_io_ext::{LimitedUdpCopyRemoteRecv, LimitedUdpCopyRemoteSend};

use super::StackSocks5Escaper;
use crate::escape::EgressNotes;
use crate::module::udp_relay::{
    ArcUdpRelayTaskRemoteStats, UdpRelayRemoteWrapperStats, UdpRelaySetupResult, UdpRelayTaskConf,
};
use crate::serve::ServerTaskNotes;

mod recv;
mod send;

pub(crate) use recv::StackSocks5UdpRelayRemoteRecv;
pub(crate) use send::StackSocks5UdpRelayRemoteSend;

impl StackSocks5Escaper {
    pub(super) async fn udp_setup_relay(
        &self,
        _task_conf: &UdpRelayTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpRelayTaskRemoteStats,
    ) -> UdpRelaySetupResult {
        let (ctl_stream, udp_c) = self
            .timed_socks5_udp_associate(egress_notes, task_notes)
            .await?;

        let mut wrapper_stats = UdpRelayRemoteWrapperStats::new(self.stats.clone(), task_stats);
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

        let udp_local_addr = egress_notes.udp.local.unwrap();
        let udp_peer_addr = egress_notes.udp.peer.unwrap();
        let recv = StackSocks5UdpRelayRemoteRecv::new(
            recv,
            udp_local_addr,
            udp_peer_addr,
            ctl_stream,
            self.config.end_on_control_closed,
            self.escape_logger.clone(),
        );
        let send = StackSocks5UdpRelayRemoteSend::new(
            send,
            udp_local_addr,
            udp_peer_addr,
            self.escape_logger.clone(),
        );

        Ok((Box::new(recv), Box::new(send)))
    }
}
