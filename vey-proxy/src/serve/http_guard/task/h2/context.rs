/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::anyhow;
use bytes::Bytes;
use h2::client::SendRequest;
use h2::server::SendResponse;
use http::{Response, StatusCode, Version};
use tokio::sync::oneshot;

use vey_daemon::stat::remote::ArcTcpConnectionTaskRemoteStats;
use vey_daemon::stat::task::TcpStreamTaskStats;
use vey_types::net::{AlpnProtocol, HttpForwardedHeaderType, HttpForwardedHeaderValue};

use super::super::CommonTaskContext;
use super::error::H2StreamTransferError;
use super::ping;
use crate::audit::AuditContext;
use crate::escape::EgressNotes;
use crate::module::http_header::{self, ProxyErrorType};
use crate::module::tcp_connect::{TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::ServerTaskNotes;
use crate::site::SiteContext;

#[derive(Clone)]
pub(crate) struct H2TaskContext {
    pub(crate) common: CommonTaskContext,
    pub(crate) site_ctx: SiteContext,
}

impl Deref for H2TaskContext {
    type Target = CommonTaskContext;

    fn deref(&self) -> &Self::Target {
        &self.common
    }
}

pub(super) struct OriginH2Sender {
    pub(crate) sender: SendRequest<Bytes>,
    pub(crate) reused: bool,
    pub(crate) egress_notes: EgressNotes,
}

impl H2TaskContext {
    pub(super) fn append_forwarded(&self, headers: &mut http::HeaderMap) {
        match self.server_config.append_forwarded_for {
            HttpForwardedHeaderType::Disable => {}
            HttpForwardedHeaderType::Classic => {
                HttpForwardedHeaderValue::new_classic(self.client_ip()).append_to_http(headers);
            }
            HttpForwardedHeaderType::Standard => {
                HttpForwardedHeaderValue::new_standard(self.client_addr(), self.server_addr())
                    .append_to_http(headers);
            }
        }
    }

    pub(super) fn local_error_response(
        &self,
        status: StatusCode,
        error: ProxyErrorType,
    ) -> Option<Response<()>> {
        if self.server_config.no_proxy_status {
            return Response::builder()
                .status(status)
                .version(Version::HTTP_2)
                .body(())
                .ok();
        }
        let ident = self
            .server_config
            .server_id
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(http_header::DEFAULT_PROXY_STATUS_IDENT);
        Response::builder()
            .status(status)
            .version(Version::HTTP_2)
            .header(
                "proxy-status",
                http_header::proxy_status_value(ident, error),
            )
            .body(())
            .ok()
    }

    pub(super) fn reply_early_error(
        &self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        status: StatusCode,
        error: ProxyErrorType,
    ) {
        let _ = self.reply_local_error(clt_send_rsp, status, error);
    }

    pub(super) fn reply_local_error(
        &self,
        clt_send_rsp: &mut SendResponse<Bytes>,
        status: StatusCode,
        error: ProxyErrorType,
    ) -> Option<u16> {
        let rsp = self.local_error_response(status, error)?;
        let rsp_status = rsp.status().as_u16();
        clt_send_rsp.send_response(rsp, true).ok()?;
        Some(rsp_status)
    }

    pub(super) async fn checkout_or_connect(
        &self,
        task_notes: &ServerTaskNotes,
    ) -> Result<OriginH2Sender, H2StreamTransferError> {
        let site = self.site_ctx.site();
        let is_tls = site.tls_client().is_some();
        let open_timeout = self.server_config.h2.upstream_stream_open_timeout;
        if let Some((sender, egress_notes)) = site
            .http2_pool()
            .checkout(
                task_notes.worker_id(),
                is_tls,
                self.escaper.name(),
                open_timeout,
            )
            .await
        {
            return Ok(OriginH2Sender {
                sender,
                reused: true,
                egress_notes,
            });
        }

        let (sender, closed, egress_notes) = self.connect_origin(task_notes).await?;
        site.http2_pool().insert(
            task_notes.worker_id(),
            is_tls,
            self.escaper.name().clone(),
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
        &self,
        task_notes: &ServerTaskNotes,
    ) -> Result<(SendRequest<Bytes>, Arc<AtomicBool>, EgressNotes), H2StreamTransferError> {
        let site = self.site_ctx.site();
        let mut egress_notes = EgressNotes::default();
        let mut audit_ctx = AuditContext::new(self.audit_handle.clone());
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
            self.escaper
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
            self.escaper
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
        self.server_config
            .h2
            .apply_to_client_builder(&mut client_builder);

        let (sender, mut connection) = tokio::time::timeout(
            self.server_config.h2.upstream_handshake_timeout,
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
        let ping_interval = self.server_config.h2.ping_interval;
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
}
