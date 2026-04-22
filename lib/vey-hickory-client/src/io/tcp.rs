/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::anyhow;
use hickory_net::runtime::DnsTcpStream;
use hickory_net::runtime::iocompat::AsyncIoTokioAsStd;
use hickory_net::tcp::{TcpClientStream, TcpStream};
use hickory_net::xfer::StreamReceiver;

use vey_socket::TcpConnectInfo;

pub async fn connect(
    connect_info: TcpConnectInfo,
    outbound_messages: StreamReceiver,
    connect_timeout: Duration,
) -> anyhow::Result<TcpClientStream<impl DnsTcpStream>> {
    let tls_stream = tokio::time::timeout(connect_timeout, connect_info.tcp_connect())
        .await
        .map_err(|_| anyhow!("tcp connect to {} timed out", connect_info.server))??;

    let stream = TcpStream::from_stream_with_receiver(
        AsyncIoTokioAsStd(tls_stream),
        connect_info.server,
        outbound_messages,
    );
    Ok(TcpClientStream::from_stream(stream))
}
