/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use vey_daemon::stat::remote::TcpConnectionTaskRemoteStatsWrapper;
use vey_io_ext::{AsyncStream, LimitedBufReader, LimitedReader, LimitedWriter, NilLimitedStats};
use vey_types::net::AlpnProtocol;

use super::{ProxySocks5sEscaper, ProxySocks5sEscaperStats};
use crate::escape::direct_fixed::http_forward::{DirectHttpForwardReader, DirectHttpForwardWriter};
use crate::escape::{ArcEscaper, EgressNotes, TlsHttpConnection};
use crate::log::escape::tls_handshake::TlsApplication;
use crate::module::http_forward::{
    ArcHttpForwardTaskRemoteStats, BoxHttpForwardConnection, HttpForwardTaskRemoteWrapperStats,
};
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;

impl ProxySocks5sEscaper {
    pub(super) async fn http_forward_new_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        let ups_s = self
            .timed_socks5_connect_tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;
        let (ups_r, ups_w) = ups_s.into_split();

        // add task and user stats
        let mut wrapper_stats = HttpForwardTaskRemoteWrapperStats::new(task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let ups_r = LimitedBufReader::new_unlimited(
            ups_r,
            Arc::new(NilLimitedStats::default()),
            wrapper_stats.clone(),
        );
        let ups_w = LimitedWriter::new(ups_w, wrapper_stats);

        let writer = DirectHttpForwardWriter::<_, ProxySocks5sEscaperStats>::new(ups_w, None);
        let reader = DirectHttpForwardReader::new(ups_r);
        Ok((Box::new(writer), Box::new(reader)))
    }

    pub(super) async fn https_forward_new_connection(
        &self,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
        task_stats: ArcHttpForwardTaskRemoteStats,
    ) -> Result<BoxHttpForwardConnection, TcpConnectError> {
        let tls_stream = self
            .socks5_connect_tls_connect_to(
                task_conf,
                egress_notes,
                task_notes,
                TlsApplication::HttpForward,
            )
            .await?;

        let (ups_r, ups_w) = tls_stream.into_split();

        // add task and user stats
        let mut wrapper_stats = HttpForwardTaskRemoteWrapperStats::new(task_stats);
        wrapper_stats.push_user_io_stats(self.fetch_user_upstream_io_stats(task_notes));
        let wrapper_stats = Arc::new(wrapper_stats);

        let ups_r = LimitedBufReader::new_unlimited(
            ups_r,
            Arc::new(NilLimitedStats::default()),
            wrapper_stats.clone(),
        );
        let ups_w = LimitedWriter::new(ups_w, wrapper_stats);

        let writer = DirectHttpForwardWriter::<_, ProxySocks5sEscaperStats>::new(ups_w, None);
        let reader = DirectHttpForwardReader::new(ups_r);
        Ok((Box::new(writer), Box::new(reader)))
    }

    pub(super) async fn open_tls_http_connection(
        &self,
        escaper: ArcEscaper,
        task_conf: &TlsConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<TlsHttpConnection, TcpConnectError> {
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
                    Box::new(DirectHttpForwardWriter::<_, ProxySocks5sEscaperStats>::new(
                        ups_w, None,
                    )),
                    Box::new(DirectHttpForwardReader::new(ups_r)),
                ),
                escaper,
            ))
        }
    }
}
