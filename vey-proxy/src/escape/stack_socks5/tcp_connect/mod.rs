/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use tokio::io::{AsyncRead, AsyncWrite};

use vey_io_ext::{LimitedReader, LimitedWriter};

use super::StackSocks5Escaper;
use crate::escape::{EgressNotes, EgressSocketType};
use crate::module::tcp_connect::{NestedTcpConnectError, TcpConnectTaskConf, TcpConnection};
use crate::serve::ServerTaskNotes;

impl StackSocks5Escaper {
    async fn tcp_connect_to(
        &self,
        task_conf: &TcpConnectTaskConf<'_>,
        egress_notes: &mut EgressNotes,
        task_notes: &ServerTaskNotes,
    ) -> Result<TcpConnection, NestedTcpConnectError> {
        egress_notes.socket_type = Some(EgressSocketType::Direct);

        if let Some(ups) = task_notes.egress_path_upstream(&self.config.name)
            && let Some(addr) = &ups.addr
        {
            egress_notes.override_peer = Some(addr.clone());
            let task_conf = TcpConnectTaskConf {
                upstream: addr,
                peer_health_check: self.config.peer_health_check,
            };
            return self
                .next
                ._nested_tcp_connect(&task_conf, egress_notes, task_notes)
                .await;
        }

        let peer_proxy = self.get_next_proxy(task_notes, task_conf.upstream.host());
        let task_conf = TcpConnectTaskConf {
            upstream: peer_proxy,
            peer_health_check: self.config.peer_health_check,
        };
        self.next
            ._nested_tcp_connect(&task_conf, egress_notes, task_notes)
            .await
    }

    pub(super) async fn tcp_new_connection(
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
        let stream = self
            .tcp_connect_to(task_conf, egress_notes, task_notes)
            .await?;

        let limit_config = &self.config.general.tcp_sock_speed_limit;
        let ups_r = LimitedReader::local_limited(
            stream.0,
            limit_config.shift_millis,
            limit_config.max_south,
            self.stats.clone(),
        );
        let ups_w = LimitedWriter::local_limited(
            stream.1,
            limit_config.shift_millis,
            limit_config.max_north,
            self.stats.clone(),
        );

        Ok((ups_r, ups_w))
    }
}
