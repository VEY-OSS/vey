/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_daemon::stat::remote::{
    ArcTcpConnectionTaskRemoteStats, TcpConnectionTaskRemoteStatsWrapper,
};
use vey_io_ext::{AsyncStream, LimitedReader, LimitedWriter};
use vey_openssl::{SslConnector, SslStream};
use vey_socks::v5;
use vey_types::net::UpstreamAddr;

use super::StackSocks5Escaper;
use crate::escape::{EgressNotes, EgressSocketType};
use crate::log::escape::tls_handshake::{EscapeLogForTlsHandshake, TlsApplication};
use crate::module::tcp_connect::{
    NestedTcpConnectError, NestedTcpConnectResult, TcpConnectError, TcpConnectRemoteWrapperStats,
    TcpConnectResult, TcpConnectTaskConf, TlsConnectTaskConf,
};
use crate::module::udp_connect::{UdpConnectError, UdpConnectTaskConf, UdpConnection};
use crate::serve::ServerTaskNotes;

impl StackSocks5Escaper {
    async fn socks5_connect_tcp_connect_to(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<
        (
            LimitedReader<impl AsyncRead + use<>>,
            LimitedWriter<impl AsyncWrite + use<>>,
        ),
        NestedTcpConnectError,
    > {
        let (ups_r, ups_w) = self
            .tcp_new_connection(task_conf, egress_notes, task_notes)
            .await?;
        egress_notes.socket_type = Some(EgressSocketType::Socks5);
        let mut stream = tokio::io::join(ups_r, ups_w);
        let outgoing_addr =
            v5::client::socks5_connect_to(&mut stream, &self.config.auth_info, task_conf.upstream)
                .await?;
        egress_notes.final_addr.outgoing_addr = Some(outgoing_addr);
        // we can not determine the real upstream addr that the proxy choose to connect to

        Ok(stream.into_split())
    }

    pub(super) async fn timed_socks5_connect_tcp_connect_to(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<
        (
            LimitedReader<impl AsyncRead + use<>>,
            LimitedWriter<impl AsyncWrite + use<>>,
        ),
        NestedTcpConnectError,
    > {
        tokio::time::timeout(
            self.config.peer_negotiation_timeout,
            self.socks5_connect_tcp_connect_to(task_conf, egress_notes, task_notes),
        )
        .await
        .map_err(|_| NestedTcpConnectError::NegotiationPeerTimeout)?
    }

    /// Open a UDP associate. The returned control stream keeps both halves so
    /// dropping it is what closes the association.
    async fn socks5_udp_associate(
        &self,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(impl AsyncRead + Unpin + Send + Sync + use<>, UdpConnection), UdpConnectError>
    {
        let tcp_task_conf = TcpConnectTaskConf {
            upstream: &UpstreamAddr::empty(),
            peer_health_check: None,
        };
        let (ups_r, ups_w) = self
            .tcp_new_connection(&tcp_task_conf, egress_notes, task_notes)
            .await?;
        let local_tcp_addr = egress_notes.tcp.local.unwrap();
        let peer_tcp_addr = egress_notes.tcp.peer.unwrap();

        let send_udp_ip = match local_tcp_addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        };
        let send_udp_addr = SocketAddr::new(send_udp_ip, 0);

        let mut ctl_stream = tokio::io::join(ups_r, ups_w);
        let peer_udp_addr = v5::client::socks5_udp_associate(
            &mut ctl_stream,
            &self.config.auth_info,
            send_udp_addr,
        )
        .await?;
        let peer_udp_addr = self
            .config
            .transmute_udp_peer_addr(peer_udp_addr, peer_tcp_addr.ip());
        egress_notes.udp.peer = Some(peer_udp_addr);

        let upstream = UpstreamAddr::from(peer_udp_addr);
        let udp_task_conf = UdpConnectTaskConf {
            upstream: &upstream,
            relay: Default::default(),
        };
        let udp_c = self
            .next
            ._nested_udp_connect(&udp_task_conf, egress_notes, task_notes)
            .await?;
        if egress_notes.udp.local.is_none() {
            egress_notes.udp.local = Some(SocketAddr::new(local_tcp_addr.ip(), 0));
        }
        egress_notes.socket_type = Some(EgressSocketType::Socks5);

        Ok((ctl_stream, udp_c))
    }

    pub(super) async fn timed_socks5_udp_associate(
        &self,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(impl AsyncRead + Unpin + Send + Sync + use<>, UdpConnection), UdpConnectError>
    {
        tokio::time::timeout(
            self.config.peer_negotiation_timeout,
            self.socks5_udp_associate(egress_notes, task_notes),
        )
        .await
        .map_err(|_| UdpConnectError::NegotiationPeerTimeout)?
    }

    pub(super) async fn socks5_new_tcp_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> TcpConnectResult {
        let (mut ups_r, mut ups_w) = self
            .timed_socks5_connect_tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;

        let mut wrapper_stats = TcpConnectRemoteWrapperStats::new(self.stats.clone(), task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        ups_r.reset_stats(wrapper_stats.clone());
        ups_w.reset_stats(wrapper_stats);

        Ok((Box::new(ups_r), Box::new(ups_w)))
    }

    pub(super) async fn socks5_nested_tcp_connect(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> NestedTcpConnectResult {
        let (ups_r, ups_w) = self
            .timed_socks5_connect_tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;

        Ok((Box::new(ups_r), Box::new(ups_w)))
    }

    pub(super) async fn socks5_connect_tls_connect_to(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        tls_application: TlsApplication,
    ) -> Result<SslStream<impl AsyncRead + AsyncWrite + use<>>, TcpConnectError> {
        let (ups_r, ups_w) = self
            .timed_socks5_connect_tcp_connect_to(&task_conf.tcp, egress_notes, task_notes)
            .await?;

        let ssl = task_conf.build_ssl()?;
        let connector = SslConnector::new(ssl, tokio::io::join(ups_r, ups_w))
            .map_err(|e| TcpConnectError::InternalTlsClientError(anyhow::Error::new(e)))?;

        match tokio::time::timeout(task_conf.handshake_timeout(), connector.connect()).await {
            Ok(Ok(stream)) => Ok(stream),
            Ok(Err(e)) => {
                let e = anyhow::Error::new(e);
                if let Some(logger) = &self.escape_logger {
                    EscapeLogForTlsHandshake {
                        upstream: task_conf.tcp.upstream,
                        egress_notes,
                        task_id: &task_notes.id,
                        tls_name: task_conf.tls_name,
                        tls_peer: task_conf.tcp.upstream,
                        tls_application,
                    }
                    .log(logger, &e);
                }
                Err(TcpConnectError::UpstreamTlsHandshakeFailed(e))
            }
            Err(_) => {
                let e = anyhow!("upstream tls handshake timed out");
                if let Some(logger) = &self.escape_logger {
                    EscapeLogForTlsHandshake {
                        upstream: task_conf.tcp.upstream,
                        egress_notes,
                        task_id: &task_notes.id,
                        tls_name: task_conf.tls_name,
                        tls_peer: task_conf.tcp.upstream,
                        tls_application,
                    }
                    .log(logger, &e);
                }
                Err(TcpConnectError::UpstreamTlsHandshakeTimeout)
            }
        }
    }

    pub(super) async fn socks5_new_tls_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> TcpConnectResult {
        let tls_stream = self
            .socks5_connect_tls_connect_to(
                task_conf,
                egress_notes,
                task_notes,
                TlsApplication::TcpStream,
            )
            .await?;

        egress_notes.record_selected_alpn(tls_stream.ssl());
        let (ups_r, ups_w) = tls_stream.into_split();

        // add task and user stats
        let mut wrapper_stats = TcpConnectionTaskRemoteStatsWrapper::new(task_stats);
        wrapper_stats.push_other_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let ups_r = LimitedReader::new(ups_r, wrapper_stats.clone());
        let ups_w = LimitedWriter::new(ups_w, wrapper_stats);

        Ok((Box::new(ups_r), Box::new(ups_w)))
    }
}
