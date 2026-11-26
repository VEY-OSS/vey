/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_io_ext::{LimitedReader, LimitedWriter};
use vey_types::net::{ProxyProtocolEncoder, ProxyProtocolWriteError, UpstreamAddr};

use super::StackHttpEscaper;
use crate::escape::{EgressNotes, EgressSocketType};
use crate::module::tcp_connect::{NestedTcpConnectError, TcpConnectTaskConf, TcpConnection};
use crate::serve::ServerTaskNotes;

impl StackHttpEscaper {
    async fn tcp_connect_to(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<(UpstreamAddr, TcpConnection), NestedTcpConnectError> {
        egress_notes.socket_type = Some(EgressSocketType::Direct);

        let peer_proxy = match task_notes.egress_path_upstream(&self.config.name) {
            Some(ups) => {
                let addr = if let Some(addr) = &ups.addr {
                    egress_notes.override_peer = Some(addr.clone());
                    addr
                } else {
                    self.get_next_proxy(task_notes, task_conf.upstream.host())
                };
                addr.clone()
            }
            None => self
                .get_next_proxy(task_notes, task_conf.upstream.host())
                .clone(),
        };

        let task_conf = TcpConnectTaskConf {
            upstream: &peer_proxy,
            peer_health_check: self.config.peer_health_check,
        };
        let stream = self
            .next
            ._nested_tcp_connect(&task_conf, egress_notes, task_notes)
            .await?;

        Ok((peer_proxy, stream))
    }

    pub(super) async fn tcp_new_connection(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<
        (
            UpstreamAddr,
            LimitedReader<impl AsyncRead + use<>>,
            LimitedWriter<impl AsyncWrite + use<>>,
        ),
        NestedTcpConnectError,
    > {
        let (peer, stream) = self
            .tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;

        let limit_config = &self.config.general.tcp_sock_speed_limit;
        let ups_r = LimitedReader::local_limited(
            stream.0,
            limit_config.shift_millis,
            limit_config.max_south,
            self.stats.clone(),
        );
        let mut ups_w = LimitedWriter::local_limited(
            stream.1,
            limit_config.shift_millis,
            limit_config.max_north,
            self.stats.clone(),
        );

        if let Some(version) = self.config.use_proxy_protocol {
            let mut encoder = ProxyProtocolEncoder::new(version);
            let bytes = encoder
                .encode_tcp(task_notes.client_addr(), task_notes.server_addr())
                .map_err(ProxyProtocolWriteError::Encode)?;
            ups_w
                .write_all(bytes) // no need to flush data
                .await
                .map_err(ProxyProtocolWriteError::Io)?;
        }

        Ok((peer, ups_r, ups_w))
    }
}
