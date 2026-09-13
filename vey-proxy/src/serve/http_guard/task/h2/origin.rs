/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::anyhow;
use bytes::Bytes;
use h2::client::SendRequest;
use tokio::sync::oneshot;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_daemon::stat::task::TcpStreamTaskStats;
use vey_types::net::AlpnProtocol;

use super::CommonTaskContext;
use super::error::H2StreamTransferError;
use super::ping;
use crate::audit::AuditContext;
use crate::escape::EgressNotes;
use crate::module::tcp_connect::{TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;
use crate::site::Site;

pub(super) struct OriginH2Sender {
    pub(crate) sender: SendRequest<Bytes>,
    pub(crate) reused: bool,
    pub(crate) egress_notes: EgressNotes,
}

pub(super) async fn checkout_or_connect(
    ctx: &CommonTaskContext,
    site: &Site,
    task_notes: &ServerTaskNotes,
) -> Result<OriginH2Sender, H2StreamTransferError> {
    let is_tls = site.tls_client().is_some();
    let open_timeout = ctx.server_config.h2.upstream_stream_open_timeout;
    if let Some((sender, egress_notes)) = site
        .http2_pool()
        .checkout(is_tls, task_notes.worker_id(), open_timeout)
        .await
    {
        return Ok(OriginH2Sender {
            sender,
            reused: true,
            egress_notes,
        });
    }

    let (sender, closed, egress_notes) = connect_origin(ctx, site, task_notes).await?;
    site.http2_pool().insert(
        task_notes.worker_id(),
        is_tls,
        sender.clone(),
        Arc::clone(&closed),
        egress_notes.clone(),
    );
    Ok(OriginH2Sender {
        sender,
        reused: false,
        egress_notes,
    })
}

async fn connect_origin(
    ctx: &CommonTaskContext,
    site: &Site,
    task_notes: &ServerTaskNotes,
) -> Result<(SendRequest<Bytes>, Arc<AtomicBool>, EgressNotes), H2StreamTransferError> {
    let mut egress_notes = EgressNotes::default();
    let mut audit_ctx = AuditContext::new(ctx.audit_handle.clone());
    let task_stats: ArcTcpConnectionTaskRemoteStats = Arc::new(TcpStreamTaskStats::default());

    let stream = if let Some(tls_client) = site.tls_client() {
        let task_conf = TlsConnectTaskConf {
            tcp: TcpConnectTaskConf {
                upstream: site.upstream(),
            },
            tls_config: tls_client,
            tls_name: site.tls_name(),
            alpn_protocols: Some(&[AlpnProtocol::Http2]),
        };
        ctx.escaper
            .tls_setup_connection(
                &task_conf,
                &mut egress_notes,
                task_notes,
                task_stats,
                &mut audit_ctx,
            )
            .await
            .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))?
    } else {
        let task_conf = TcpConnectTaskConf {
            upstream: site.upstream(),
        };
        ctx.escaper
            .tcp_setup_connection(
                &task_conf,
                &mut egress_notes,
                task_notes,
                task_stats,
                &mut audit_ctx,
            )
            .await
            .map_err(|e| H2StreamTransferError::OriginConnectFailed(anyhow!("{e}")))?
    };

    let (ups_r, ups_w) = stream;
    let mut client_builder = h2::client::Builder::new();
    ctx.server_config
        .h2
        .apply_to_client_builder(&mut client_builder);

    let (sender, mut connection) = tokio::time::timeout(
        ctx.server_config.h2.upstream_handshake_timeout,
        client_builder.handshake(tokio::io::join(ups_r, ups_w)),
    )
    .await
    .map_err(|_| {
        H2StreamTransferError::OriginConnectFailed(anyhow!("upstream h2 handshake timeout"))
    })?
    .map_err(|e| {
        H2StreamTransferError::OriginConnectFailed(anyhow!("upstream h2 handshake: {e}"))
    })?;

    let closed = Arc::new(AtomicBool::new(false));
    let (ping_quit_tx, ping_quit_rx) = oneshot::channel();
    let ping_interval = ctx.server_config.h2.ping_interval;
    if !ping_interval.is_zero()
        && let Some(ping) = connection.ping_pong()
    {
        ping::spawn_ping(ping, ping_interval, ping_quit_rx);
    } else {
        drop(ping_quit_rx);
    }

    let closed_flag = Arc::clone(&closed);
    tokio::spawn(async move {
        let _ = connection.await;
        closed_flag.store(true, Ordering::Release);
        let _ = ping_quit_tx.send(());
    });

    Ok((sender, closed, egress_notes))
}
