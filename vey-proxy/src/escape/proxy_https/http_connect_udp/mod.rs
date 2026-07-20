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

use super::ProxyHttpsEscaper;
use crate::escape::proxy_http::{ProxyHttpConnectUdpRecv, ProxyHttpConnectUdpSend};
use crate::escape::{EgressNotes, EgressSocketType};
use crate::module::tcp_connect::TcpConnectTaskConf;
use crate::module::udp_connect::{
    UdpConnectError, UdpConnectRemoteWrapperStats, UdpConnectResult, UdpConnectTaskConf,
};
use crate::serve::ServerTaskNotes;

impl ProxyHttpsEscaper {
    async fn masque_udp_connect_to(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<FlexBufReader<SslStream<impl AsyncRead + AsyncWrite + use<>>>, UdpConnectError>
    {
        let tcp_task_conf = TcpConnectTaskConf {
            upstream: task_conf.upstream,
        };
        let (peer, mut stream) = self
            .tls_handshake_to_remote(&tcp_task_conf, egress_notes, task_notes)
            .await?;

        egress_notes.socket_type = Some(EgressSocketType::Http);
        let mut req = HttpUpgradeRequest::new(&peer, &self.config.append_http_headers);

        if self.config.pass_proxy_userid
            && let Some(name) = task_notes.raw_user_name()
        {
            let line = crate::module::http_header::proxy_authorization_basic_pass(name);
            req.append_dyn_header(line);
        }

        req.send_connect_udp(task_conf.upstream, &mut stream)
            .await
            .map_err(UdpConnectError::NegotiationWriteFailed)?;

        let mut buf_stream = FlexBufReader::new(stream);
        let _ = HttpUpgradeResponse::recv_for_connect_udp(
            &mut buf_stream,
            self.config.http_connect_rsp_hdr_max_size,
        )
        .await?;

        // TODO detect and set outgoing_addr and target_addr for supported remote proxies

        Ok(buf_stream)
    }

    async fn timed_masque_udp_connect_to(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<FlexBufReader<SslStream<impl AsyncRead + AsyncWrite + use<>>>, UdpConnectError>
    {
        tokio::time::timeout(
            self.config.peer_negotiation_timeout,
            self.masque_udp_connect_to(task_conf, egress_notes, task_notes),
        )
        .await
        .map_err(|_| UdpConnectError::NegotiationPeerTimeout)?
    }

    pub(super) async fn http_upgrade_new_udp_connection(
        &self,
        task_conf: &UdpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcUdpConnectTaskRemoteStats,
    ) -> UdpConnectResult {
        let buf_stream = self
            .timed_masque_udp_connect_to(task_conf, egress_notes, task_notes)
            .await?;

        let mut wrapper_stats = UdpConnectRemoteWrapperStats::new_layered(task_stats);
        let user_stats = self.fetch_user_upstream_io_stats(task_notes);
        wrapper_stats.push_user_io_stats(user_stats);
        let wrapper_stats = Arc::new(wrapper_stats);

        let (r, w) = buf_stream.into_split();
        let recv = ProxyHttpConnectUdpRecv::new(
            r,
            self.escape_logger.clone(),
            task_conf.relay.underlying_buffer_size(),
            task_conf.relay.packet_size(),
        );
        let send = ProxyHttpConnectUdpSend::new(
            w,
            self.escape_logger.clone(),
            task_conf.relay.packet_size(),
        );

        let recv = LimitedUdpCopyRemoteRecv::unlimited(recv, wrapper_stats.clone());
        let send = LimitedUdpCopyRemoteSend::unlimited(send, wrapper_stats);

        Ok((Box::new(recv), Box::new(send)))
    }
}
