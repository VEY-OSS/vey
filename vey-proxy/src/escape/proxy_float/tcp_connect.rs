/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::SocketAddr;

use tokio::net::TcpStream;
use tokio::time::Instant;

use vey_io_ext::LimitedStream;
use vey_socket::BindAddr;
use vey_types::net::ConnectError;

use super::{NextProxyPeer, ProxyFloatEscaper};
use crate::escape::{EgressNotes, EgressSocketType};
use crate::log::escape::tcp_connect::EscapeLogForTcpConnect;
use crate::module::tcp_connect::{TcpConnectTaskConf, UnderlyingTcpConnectError};
use crate::serve::ServerTaskNotes;

impl ProxyFloatEscaper {
    async fn try_connect_tcp(
        &self,
        peer: SocketAddr,
        bind: &BindAddr,
    ) -> Result<TcpStream, UnderlyingTcpConnectError> {
        // use new socket every time, as we set bind_no_port
        let sock = vey_socket::tcp::new_socket_to(
            peer.ip(),
            bind,
            &self.config.tcp_keepalive,
            &self.config.tcp_misc_opts,
            true,
        )
        .map_err(UnderlyingTcpConnectError::SetupSocketFailed)?;
        self.stats.tcp.connect.add_attempted();
        match sock.connect(peer).await {
            Ok(ups_stream) => Ok(ups_stream),
            Err(e) => Err(UnderlyingTcpConnectError::ConnectFailed(
                ConnectError::from(e),
            )),
        }
    }

    async fn tcp_connect_to<P: NextProxyPeer>(
        &self,
        peer: &P,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<TcpStream, UnderlyingTcpConnectError> {
        egress_notes.socket_type = Some(EgressSocketType::Tcp);
        let peer_addr = peer.peer_addr();
        egress_notes.tcp.peer = Some(peer_addr);
        let bind_ip = match peer_addr {
            SocketAddr::V4(_) => self.config.bind_v4,
            SocketAddr::V6(_) => self.config.bind_v6,
        };
        #[cfg(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "illumos",
            target_os = "solaris"
        ))]
        let bind = bind_ip.map(BindAddr::Ip).unwrap_or_else(|| {
            self.config
                .bind_interface
                .map(BindAddr::Interface)
                .unwrap_or_default()
        });
        #[cfg(not(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "illumos",
            target_os = "solaris"
        )))]
        let bind = bind_ip.map(BindAddr::Ip).unwrap_or_default();
        egress_notes.bind = bind;
        egress_notes.expire = peer.expire_datetime();
        egress_notes.egress = Some(peer.egress_info());
        egress_notes.tries = 1;
        let instant_now = Instant::now();
        let ret = tokio::time::timeout(
            self.config.tcp_connect_timeout,
            self.try_connect_tcp(peer_addr, &egress_notes.bind),
        )
        .await;
        egress_notes.duration = instant_now.elapsed();
        match ret {
            Ok(Ok(ups_stream)) => {
                self.stats.tcp.connect.add_success();

                let local_addr = ups_stream
                    .local_addr()
                    .map_err(UnderlyingTcpConnectError::SetupSocketFailed)?;
                self.stats.tcp.connect.add_established();
                egress_notes.tcp.local = Some(local_addr);
                Ok(ups_stream)
            }
            Ok(Err(e)) => {
                self.stats.tcp.connect.add_error();
                if let Some(logger) = &self.escape_logger {
                    EscapeLogForTcpConnect {
                        upstream: task_conf.upstream,
                        egress_notes,
                        task_id: &task_notes.id,
                    }
                    .log(logger, &e);
                }
                Err(e)
            }
            Err(_) => {
                self.stats.tcp.connect.add_timeout();

                let e = UnderlyingTcpConnectError::TimeoutByRule;
                if let Some(logger) = &self.escape_logger {
                    EscapeLogForTcpConnect {
                        upstream: task_conf.upstream,
                        egress_notes,
                        task_id: &task_notes.id,
                    }
                    .log(logger, &e);
                }
                Err(e)
            }
        }
    }

    pub(super) async fn tcp_new_connection<P: NextProxyPeer>(
        &self,
        peer: &P,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<LimitedStream<TcpStream>, UnderlyingTcpConnectError> {
        let stream = self
            .tcp_connect_to(peer, task_conf, egress_notes, task_notes)
            .await?;

        let limit_config = peer.tcp_sock_speed_limit();
        let stream = LimitedStream::local_limited(
            stream,
            limit_config.shift_millis,
            limit_config.max_south,
            limit_config.max_north,
            self.stats.clone(),
        );

        Ok(stream)
    }
}
