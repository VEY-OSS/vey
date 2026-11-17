/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::io;
use std::sync::Arc;

use tokio::net::TcpStream;

use vey_io_ext::LimitedStream;
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd"
))]
use vey_socket::BindAddr;
use vey_types::net::{Host, TcpKeepAliveConfig, UpstreamAddr};

use super::DirectFloatEscaper;
use crate::config::escaper::direct_float::DirectFloatBindIp;
use crate::escape::direct_fixed::tcp_connect::DirectTcpConnectConfig;
use crate::escape::{EgressNotes, EgressSocketType};
use crate::module::ftp_over_http::{
    ArcFtpTaskRemoteControlStats, ArcFtpTaskRemoteTransferStats, BoxFtpRemoteConnection,
    FtpControlRemoteWrapperStats, FtpTransferRemoteWrapperStats,
};
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf, UnderlyingTcpConnectError};
use crate::serve::ServerTaskNotes;

impl DirectFloatEscaper {
    async fn ftp_transfer_connect_to(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        server: &UpstreamAddr,
        egress_notes: &mut EgressNotes,
        ctl_egress_notes: &EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(TcpStream, DirectFloatBindIp), UnderlyingTcpConnectError> {
        egress_notes.socket_type = Some(EgressSocketType::Direct);
        egress_notes.bind = ctl_egress_notes.bind;
        #[cfg(any(
            target_os = "linux",
            target_os = "freebsd",
            target_os = "openbsd",
            target_os = "netbsd"
        ))]
        if matches!(egress_notes.bind, BindAddr::Foreign(_)) {
            return Err(UnderlyingTcpConnectError::InternalServerError(
                "foreign ip address binding is not supported",
            ));
        }

        let mut config = DirectTcpConnectConfig {
            connect: self.config.general.tcp_connect,
            // tcp keepalive is not needed for ftp transfer connection as it shouldn't be idle
            keepalive: TcpKeepAliveConfig::default(),
            misc_opts: Cow::Borrowed(&self.config.tcp_misc_opts),
        };

        // no site-ctx check here as site don't support ftp-over-http
        if let Some(user_ctx) = task_notes.user_ctx() {
            if let Some(user_config) = &user_ctx.user_config().tcp_connect {
                config.connect.limit_to(user_config);
            }

            config.misc_opts = user_ctx
                .user_config()
                .tcp_remote_misc_opts(&self.config.tcp_misc_opts);
        }

        let peer_ip = if task_conf.upstream.host_eq(server) {
            // This escaper only set tcp.peer for TCP connections
            ctl_egress_notes
                .tcp
                .peer
                .ok_or_else(|| {
                    UnderlyingTcpConnectError::SetupSocketFailed(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "no peer address for referenced connection found",
                    ))
                })?
                .ip()
        } else {
            match task_conf.upstream.host() {
                Host::Ip(ip) => *ip,
                Host::Domain(_) => {
                    return Err(UnderlyingTcpConnectError::SetupSocketFailed(
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "ftp transfer peer address should not be a different domain",
                        ),
                    ));
                }
            }
        };

        self.fixed_try_connect(peer_ip, config, task_conf, egress_notes, task_notes)
            .await
    }

    pub(super) async fn new_ftp_control_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteControlStats,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        let (stream, _) = self
            .tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;

        let mut wrapper_stats = FtpControlRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let limit_config = &self.config.general.tcp_sock_speed_limit;
        let stream = LimitedStream::local_limited(
            stream,
            limit_config.shift_millis,
            limit_config.max_south,
            limit_config.max_north,
            wrapper_stats,
        );

        Ok(Box::new(stream))
    }

    pub(super) async fn new_ftp_transfer_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        transfer_egress_notes: &mut EgressNotes,
        control_egress_notes: &EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcFtpTaskRemoteTransferStats,
        ftp_server: &UpstreamAddr,
    ) -> Result<BoxFtpRemoteConnection, TcpConnectError> {
        let (stream, _) = self
            .ftp_transfer_connect_to(
                task_conf,
                ftp_server,
                transfer_egress_notes,
                control_egress_notes,
                task_notes,
            )
            .await?;

        let mut wrapper_stats = FtpTransferRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let limit_config = &self.config.general.tcp_sock_speed_limit;
        let stream = LimitedStream::local_limited(
            stream,
            limit_config.shift_millis,
            limit_config.max_south,
            limit_config.max_north,
            wrapper_stats,
        );

        Ok(Box::new(stream))
    }
}
