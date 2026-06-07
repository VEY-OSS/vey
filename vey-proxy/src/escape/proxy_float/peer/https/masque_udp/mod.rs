/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use vey_daemon::stat::remote::ArcUdpConnectTaskRemoteStats;
use vey_http::upgrade::{HttpUpgradeRequest, HttpUpgradeResponse};
use vey_io_ext::{AsyncStream, FlexBufReader, LimitedUdpCopyRemoteRecv, LimitedUdpCopyRemoteSend};
use vey_openssl::SslStream;

use super::ProxyFloatHttpsPeer;
use crate::escape::proxy_float::ProxyFloatEscaper;
use crate::escape::proxy_http::{ProxyHttpMasqueUdpRecv, ProxyHttpMasqueUdpSend};
use crate::module::tcp_connect::{TcpConnectTaskConf, TcpConnectTaskNotes};
use crate::module::udp_connect::{
    UdpConnectError, UdpConnectRemoteWrapperStats, UdpConnectResult, UdpConnectTaskConf,
    UdpConnectTaskNotes,
};
use crate::serve::ServerTaskNotes;

impl ProxyFloatHttpsPeer {
    async fn masque_udp_connect_to(
        &self,
        escaper: &ProxyFloatEscaper,
        task_conf: &UdpConnectTaskConf<'_>,
        udp_notes: &mut UdpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<FlexBufReader<SslStream<impl AsyncRead + AsyncWrite + use<>>>, UdpConnectError>
    {
        let tcp_task_conf = TcpConnectTaskConf {
            upstream: task_conf.upstream,
        };
        let mut tcp_notes = TcpConnectTaskNotes::default();
        let mut stream = escaper
            .tls_handshake_with_peer(
                &tcp_task_conf,
                &mut tcp_notes,
                task_notes,
                &self.tls_name,
                self,
            )
            .await?;
        udp_notes.fill_from_underlying_tcp(tcp_notes);

        let req = HttpUpgradeRequest::new(&self.http_host, &self.shared_config.append_http_headers);
        req.send_connect_udp(task_conf.upstream, &mut stream)
            .await
            .map_err(UdpConnectError::NegotiationWriteFailed)?;

        let mut buf_stream = FlexBufReader::new(stream);
        let _ = HttpUpgradeResponse::recv_for_connect_udp(
            &mut buf_stream,
            self.http_connect_rsp_hdr_max_size,
        )
        .await?;

        // TODO detect and set outgoing_addr and target_addr for supported remote proxies

        Ok(buf_stream)
    }

    async fn timed_masque_udp_connect_to(
        &self,
        escaper: &ProxyFloatEscaper,
        task_conf: &UdpConnectTaskConf<'_>,
        udp_notes: &mut UdpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<FlexBufReader<SslStream<impl AsyncRead + AsyncWrite + use<>>>, UdpConnectError>
    {
        tokio::time::timeout(
            escaper.config.peer_negotiation_timeout,
            self.masque_udp_connect_to(escaper, task_conf, udp_notes, task_notes),
        )
        .await
        .map_err(|_| UdpConnectError::NegotiationPeerTimeout)?
    }

    pub(super) async fn http_upgrade_new_udp_connection(
        &self,
        escaper: &ProxyFloatEscaper,
        task_conf: &UdpConnectTaskConf<'_>,
        udp_notes: &mut UdpConnectTaskNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        let buf_stream = self
            .timed_masque_udp_connect_to(escaper, task_conf, udp_notes, task_notes)
            .await?;

        let mut wrapper_stats = UdpConnectRemoteWrapperStats::new_layered(task_stats);
        let user_stats = escaper.fetch_user_upstream_io_stats(task_notes);
        wrapper_stats.push_user_io_stats(user_stats);
        let wrapper_stats = Arc::new(wrapper_stats);

        let (r, w) = buf_stream.into_split();
        let recv = ProxyHttpMasqueUdpRecv::new(
            r,
            escaper.escape_logger.clone(),
            task_conf.relay.underlying_buffer_size(),
            task_conf.relay.packet_size(),
        );
        let send = ProxyHttpMasqueUdpSend::new(
            w,
            escaper.escape_logger.clone(),
            task_conf.relay.packet_size(),
        );

        let recv = LimitedUdpCopyRemoteRecv::unlimited(recv, wrapper_stats.clone());
        let send = LimitedUdpCopyRemoteSend::unlimited(send, wrapper_stats);

        Ok((Box::new(recv), Box::new(send)))
    }
}
