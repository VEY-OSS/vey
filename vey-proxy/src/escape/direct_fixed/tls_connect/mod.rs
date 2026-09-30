/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_daemon::stat::remote::{
    ArcTcpConnectionTaskRemoteStats, TcpConnectionTaskRemoteStatsWrapper,
};
use vey_io_ext::{
    AsyncStream, LimitedBufReader, LimitedReader, LimitedStream, LimitedWriter, NilLimitedStats,
};
use vey_openssl::{SslConnector, SslStream};
use vey_types::net::AlpnProtocol;

use super::http_forward::{DirectHttpForwardReader, DirectHttpForwardWriter};
use super::{DirectFixedEscaper, DirectFixedEscaperStats};
use crate::escape::{ArcEscaper, EgressNotes, TlsHttpConnection};
use crate::log::escape::tls_handshake::{EscapeLogForTlsHandshake, TlsApplication};
use crate::module::http_forward::HttpForwardTaskRemoteWrapperStats;
use crate::module::tcp_connect::{TcpConnectError, TcpConnectResult, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;

impl DirectFixedEscaper {
    pub(super) async fn tls_connect_to(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        tls_application: TlsApplication,
    ) -> Result<SslStream<impl AsyncRead + AsyncWrite + use<>>, TcpConnectError> {
        let mut stream = self
            .tcp_connect_to(&task_conf.tcp, egress_notes, task_notes)
            .await?;
        if let Some(version) = self.config.use_proxy_protocol {
            self.send_tcp_proxy_protocol_header(version, &mut stream, task_notes, false)
                .await?;
        }

        // set limit config and add escaper stats, do not count in task stats
        let limit_config = &self.config.general.tcp_sock_speed_limit;
        let stream = LimitedStream::local_limited(
            stream,
            limit_config.shift_millis,
            limit_config.max_south,
            limit_config.max_north,
            self.stats.clone(),
        );

        let ssl = task_conf.build_ssl()?;
        let connector = SslConnector::new(ssl, stream)
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

    pub(super) async fn tls_new_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcTcpConnectionTaskRemoteStats,
    ) -> TcpConnectResult {
        let tls_stream = self
            .tls_connect_to(
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

    pub(super) async fn open_tls_http_connection(
        &self,
        escaper: ArcEscaper,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<TlsHttpConnection, TcpConnectError> {
        let tls_stream = self
            .tls_connect_to(
                task_conf,
                egress_notes,
                task_notes,
                TlsApplication::TcpStream,
            )
            .await?;
        egress_notes.record_selected_alpn(tls_stream.ssl());
        let (ups_r, ups_w) = tls_stream.into_split();
        if egress_notes.selected_alpn == Some(AlpnProtocol::Http2) {
            let mut wrapper_stats = TcpConnectionTaskRemoteStatsWrapper::default();
            wrapper_stats.push_other_stats(self.fetch_user_upstream_io_stats(task_notes));
            let wrapper_stats = Arc::new(wrapper_stats);
            Ok(TlsHttpConnection::H2((
                Box::new(LimitedReader::new(ups_r, wrapper_stats.clone())),
                Box::new(LimitedWriter::new(ups_w, wrapper_stats)),
            )))
        } else {
            let mut wrapper_stats = HttpForwardTaskRemoteWrapperStats::default();
            wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
            let wrapper_stats = Arc::new(wrapper_stats);
            let ups_r = LimitedBufReader::new_unlimited(
                ups_r,
                Arc::new(NilLimitedStats::default()),
                wrapper_stats.clone(),
            );
            let ups_w = LimitedWriter::new(ups_w, wrapper_stats);
            Ok(TlsHttpConnection::H1(
                (
                    Box::new(DirectHttpForwardWriter::<_, DirectFixedEscaperStats>::new(
                        ups_w, None,
                    )),
                    Box::new(DirectHttpForwardReader::new(ups_r)),
                ),
                escaper,
            ))
        }
    }
}
