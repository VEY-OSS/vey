/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;
use vey_io_ext::{LimitedUdpRecv, LimitedUdpSend};
use vey_socket::BindAddr;
use vey_socket::util::AddressFamily;
use vey_types::acl::AclAction;

use super::DirectFloatEscaper;
use crate::escape::direct_fixed::udp_connect::{
    DirectUdpConnectRemoteRecv, DirectUdpConnectRemoteSend,
};
use crate::escape::{EgressNotes, EgressSocketType};
use crate::module::udp_connect::{
    UdpConnectError, UdpConnectRemoteWrapperStats, UdpConnectResult, UdpConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

impl DirectFloatEscaper {
    fn handle_udp_target_ip_acl_action(
        &self,
        action: AclAction,
        task_notes: &ServerTaskNotes,
    ) -> Result<(), UdpConnectError> {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log
                true
            }
        };
        if forbid {
            self.stats.forbidden.add_ip_blocked();
            if let Some(user_ctx) = task_notes.user_ctx() {
                user_ctx.add_ip_blocked();
            }
            Err(UdpConnectError::ForbiddenRemoteAddress)
        } else {
            Ok(())
        }
    }

    pub(super) async fn udp_connect_to(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        egress_notes.socket_type = Some(EgressSocketType::Direct);

        let peer_addr = self
            .select_upstream_addr(
                task_conf.upstream,
                self.get_resolve_strategy(task_notes),
                task_notes,
            )
            .await?;
        egress_notes.udp.peer = Some(peer_addr);

        let (_, action) = self.egress_net_filter.check(peer_addr.ip());
        self.handle_udp_target_ip_acl_action(action, task_notes)?;

        let family = AddressFamily::from(&peer_addr);
        let bind = self
            .select_bind(family, task_notes)
            .map_err(UdpConnectError::EscaperNotUsable)?;
        egress_notes.bind = BindAddr::Ip(bind.ip);
        egress_notes.expire = bind.expire_datetime;
        egress_notes.egress = Some(bind.egress_info);

        let misc_opts = if let Some(user_ctx) = task_notes.user_ctx() {
            user_ctx
                .user_config()
                .udp_remote_misc_opts(&self.config.udp_misc_opts)
        } else {
            self.config.udp_misc_opts
        };

        let (socket, local_addr) = vey_socket::udp::new_connected_to(
            peer_addr,
            &egress_notes.bind,
            self.config.udp_socket_buffer,
            misc_opts,
        )
        .map_err(UdpConnectError::SetupSocketFailed)?;
        egress_notes.udp.local = Some(local_addr);

        let mut wrapper_stats = UdpConnectRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let (recv, send) = vey_io_ext::split_udp(socket);
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

        Ok((
            Box::new(DirectUdpConnectRemoteRecv::new(
                recv,
                self.escape_logger.clone(),
            )),
            Box::new(DirectUdpConnectRemoteSend::new(
                send,
                self.escape_logger.clone(),
            )),
        ))
    }
}
