/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::sync::Arc;

use http::Version;
use tokio::io::{AsyncRead, AsyncWrite};

use vey_daemon::stat::task::UdpConnectTaskStats;
use vey_http::server::HttpProxyClientRequest;
use vey_io_ext::{
    LimitedReader, LimitedUdpCopyClientRecv, LimitedUdpCopyClientSend, LimitedWriter,
    UdpCopyClientRecv, UdpCopyClientSend, UdpCopyClientToRemote, UdpCopyError, UdpCopyRemoteRecv,
    UdpCopyRemoteSend, UdpCopyRemoteToClient,
};
use vey_types::acl::AclAction;
use vey_types::net::{ProxyRequestType, UpstreamAddr};

use super::protocol::{HttpClientWriter, HttpProxyRequest};
use super::{
    CommonTaskContext, HttpConnectUdpRecv, HttpConnectUdpSend, HttpConnectUdpTaskCltWrapperStats,
    HttpConnectUdpTaskServerCltWrapperStats,
};
use crate::config::server::ServerConfig;
use crate::log::escape::udp_sendto::EscapeLogForUdpConnectSendTo;
use crate::log::task::udp_connect::TaskLogForUdpConnect;
use crate::module::http_forward::HttpProxyClientResponse;
use crate::module::udp_connect::{
    UdpConnectError, UdpConnectTaskConf, UdpConnectTaskNotes, UdpConnection,
};
use crate::serve::http_proxy::HttpConnectUdpTaskAliveGuard;
use crate::serve::{
    ServerStats, ServerTaskError, ServerTaskForbiddenError, ServerTaskNotes, ServerTaskResult,
    ServerTaskStage,
};

pub(crate) struct HttpProxyConnectUdpTask {
    ctx: Arc<CommonTaskContext>,
    upstream: UpstreamAddr,
    ups_c: Option<UdpConnection>,
    back_to_http: bool,
    task_notes: ServerTaskNotes,
    udp_notes: UdpConnectTaskNotes,
    task_stats: Arc<UdpConnectTaskStats>,
    http_version: Version,
    max_idle_count: usize,
    started: bool,
    _alive_guard: Option<HttpConnectUdpTaskAliveGuard>,
}

impl Drop for HttpProxyConnectUdpTask {
    fn drop(&mut self) {
        if self.started {
            self.post_stop();
            self.started = false;
        }
    }
}

impl HttpProxyConnectUdpTask {
    pub(crate) fn new(
        ctx: Arc<CommonTaskContext>,
        req: &HttpProxyRequest<impl AsyncRead>,
        task_notes: ServerTaskNotes,
    ) -> Self {
        let max_idle_count = task_notes
            .user_ctx()
            .and_then(|c| c.user().task_max_idle_count())
            .unwrap_or(ctx.server_config.task_idle_max_count);
        HttpProxyConnectUdpTask {
            ctx,
            upstream: req.upstream.clone(),
            ups_c: None,
            back_to_http: false,
            task_notes,
            udp_notes: UdpConnectTaskNotes::default(),
            task_stats: Arc::new(UdpConnectTaskStats::default()),
            http_version: req.inner.version,
            max_idle_count,
            started: false,
            _alive_guard: None,
        }
    }

    async fn reply_bad_request<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let rsp = HttpProxyClientResponse::bad_request(self.http_version);
        // no custom header is set
        let _ = rsp.reply_err_to_request(clt_w).await;
        self.back_to_http = false;
    }

    async fn reply_too_many_requests<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let rsp = HttpProxyClientResponse::too_many_requests(self.http_version);
        // no custom header is set
        let _ = rsp.reply_err_to_request(clt_w).await;
        self.back_to_http = false;
    }

    async fn reply_forbidden<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let rsp = HttpProxyClientResponse::forbidden(self.http_version);
        // no custom header is set
        let _ = rsp.reply_err_to_request(clt_w).await;
        self.back_to_http = false;
    }

    async fn reply_banned_protocol<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let rsp = HttpProxyClientResponse::method_not_allowed(self.http_version);
        // no custom header is set
        let _ = rsp.reply_err_to_request(clt_w).await;
        self.back_to_http = false;
    }

    async fn reply_upgraded<W>(&self, clt_w: &mut W) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::from_standard(
            http::StatusCode::SWITCHING_PROTOCOLS,
            self.http_version,
            false,
        );
        self.ctx
            .set_custom_header_for_udp_local_reply(&self.udp_notes, &mut rsp);
        rsp.add_extra_header(String::from(
            "Connection: Upgrade\r\nUpgrade: connect-udp\r\nCapsule-Protocol: ?1\r\n",
        ));
        rsp.reply_ok_to_connect(clt_w)
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)
    }

    async fn reply_connect_err<W>(&mut self, e: &UdpConnectError, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        // If the next-hop was derived from username params and DNS failed,
        // treat it as a bad request (400) instead of origin DNS error.
        if self.udp_notes.override_peer.is_some() && matches!(e, UdpConnectError::ResolveFailed(_))
        {
            let mut rsp = HttpProxyClientResponse::bad_request(self.http_version);
            rsp.set_error_message("Proxy targeting didn't find a match");
            // no custom header is set for 400
            self.back_to_http = false;
            let _ = rsp.reply_err_to_request(clt_w).await;
            return;
        }

        let mut rsp = HttpProxyClientResponse::from_udp_connect_error(e, self.http_version, false);
        self.ctx
            .set_custom_header_for_udp_local_reply(&self.udp_notes, &mut rsp);
        let should_close = rsp.should_close();
        self.back_to_http = !should_close;

        if rsp.reply_err_to_request(clt_w).await.is_err() {
            self.back_to_http = false;
        }
    }

    pub(crate) async fn connect_to_upstream<W>(
        &mut self,
        req: &HttpProxyClientRequest,
        clt_w: &mut W,
    ) where
        W: AsyncWrite + Unpin,
    {
        let Some(v) = req.end_to_end_headers.get("capsule-protocol") else {
            self.reply_bad_request(clt_w).await;
            return;
        };
        if !v.as_bytes().starts_with(b"?1") {
            self.reply_bad_request(clt_w).await;
            return;
        }

        self.pre_start();
        match self.do_connect(clt_w).await {
            Ok(()) => {
                self.back_to_http = false;
            }
            Err(e) => {
                if let Some(log_ctx) = self.get_log_context() {
                    log_ctx.log(e);
                }
            }
        }
    }

    async fn handle_server_upstream_acl_action<W>(
        &mut self,
        action: AclAction,
        clt_w: &mut W,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log permit
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log forbid
                true
            }
        };
        if forbid {
            self.ctx.server_stats.forbidden.add_dest_denied();
            if let Some(user_ctx) = self.task_notes.user_ctx() {
                // also add to user level forbidden stats
                user_ctx.add_dest_denied();
            }

            self.reply_forbidden(clt_w).await;
            Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::DestDenied,
            ))
        } else {
            Ok(())
        }
    }

    async fn handle_user_upstream_acl_action<W>(
        &mut self,
        action: AclAction,
        clt_w: &mut W,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log permit
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log forbid
                true
            }
        };
        if forbid {
            self.reply_forbidden(clt_w).await;
            Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::DestDenied,
            ))
        } else {
            Ok(())
        }
    }

    async fn handle_user_protocol_acl_action<W>(
        &mut self,
        action: AclAction,
        clt_w: &mut W,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let forbid = match action {
            AclAction::Permit => false,
            AclAction::PermitAndLog => {
                // TODO log permit
                false
            }
            AclAction::Forbid => true,
            AclAction::ForbidAndLog => {
                // TODO log forbid
                true
            }
        };
        if forbid {
            self.reply_banned_protocol(clt_w).await;
            Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::ProtoBanned,
            ))
        } else {
            Ok(())
        }
    }

    async fn do_connect<W>(&mut self, clt_w: &mut W) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let tcp_client_misc_opts;
        if let Some(user_ctx) = self.task_notes.user_ctx() {
            let user_ctx = user_ctx.clone();

            if user_ctx.check_rate_limit().is_err() {
                self.reply_too_many_requests(clt_w).await;
                return Err(ServerTaskError::ForbiddenByRule(
                    ServerTaskForbiddenError::RateLimited,
                ));
            }

            match user_ctx.acquire_request_semaphore() {
                Ok(permit) => self.task_notes.user_req_alive_permit = Some(permit),
                Err(_) => {
                    self.reply_too_many_requests(clt_w).await;
                    return Err(ServerTaskError::ForbiddenByRule(
                        ServerTaskForbiddenError::FullyLoaded,
                    ));
                }
            }

            let action = user_ctx.check_proxy_request(ProxyRequestType::HttpConnectUdp);
            self.handle_user_protocol_acl_action(action, clt_w).await?;

            let action = user_ctx.check_upstream(&self.upstream);
            self.handle_user_upstream_acl_action(action, clt_w).await?;

            // server level dst host/port acl rules
            let action = self.ctx.check_upstream(&self.upstream);
            self.handle_server_upstream_acl_action(action, clt_w)
                .await?;

            tcp_client_misc_opts = user_ctx
                .user_config()
                .tcp_client_misc_opts(&self.ctx.server_config.tcp_misc_opts);
        } else {
            // server level dst host/port acl rules
            let action = self.ctx.check_upstream(&self.upstream);
            self.handle_server_upstream_acl_action(action, clt_w)
                .await?;

            tcp_client_misc_opts = Cow::Borrowed(&self.ctx.server_config.tcp_misc_opts);
        }

        // set client side socket options
        self.ctx
            .cc_info
            .tcp_sock_set_raw_opts(&tcp_client_misc_opts, true)
            .map_err(|_| {
                ServerTaskError::InternalServerError("failed to set client socket options")
            })?;

        self.task_notes.stage = ServerTaskStage::Connecting;

        let task_conf = UdpConnectTaskConf {
            upstream: &self.upstream,
            relay: self.ctx.server_config.udp_relay,
        };
        match self
            .ctx
            .escaper
            .udp_setup_connection(
                &task_conf,
                &mut self.udp_notes,
                &self.task_notes,
                self.task_stats.clone(),
            )
            .await
        {
            Ok(c) => {
                self.task_notes.stage = ServerTaskStage::Connected;
                self.ups_c = Some(c);
                Ok(())
            }
            Err(e) => {
                self.reply_connect_err(&e, clt_w).await;
                Err(e.into())
            }
        }
    }

    pub(crate) fn back_to_http(&self) -> bool {
        self.back_to_http
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_http_connect_udp_task());

        if let Some(user_ctx) = self.task_notes.user_ctx() {
            user_ctx.foreach_req_stats(|s| {
                s.req_total.add_http_connect_udp();
                s.req_alive.add_http_connect_udp();
            });
        }

        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_created();
        }

        self.started = true;
    }

    fn post_stop(&mut self) {
        if let Some(user_ctx) = self.task_notes.user_ctx() {
            user_ctx.foreach_req_stats(|s| {
                s.req_alive.del_http_connect_udp();
            });

            if let Some(user_req_alive_permit) = self.task_notes.user_req_alive_permit.take() {
                drop(user_req_alive_permit);
            }
        }
    }

    fn get_log_context(&self) -> Option<TaskLogForUdpConnect<'_>> {
        self.ctx
            .task_logger
            .as_ref()
            .map(|logger| TaskLogForUdpConnect {
                logger,
                task_notes: &self.task_notes,
                tcp_server_addr: Some(self.ctx.server_addr()),
                tcp_client_addr: Some(self.ctx.client_addr()),
                udp_listen_addr: None,
                udp_client_addr: None,
                upstream: Some(&self.upstream),
                udp_notes: &self.udp_notes,
                client_rd_bytes: self.task_stats.clt.recv.get_bytes(),
                client_rd_packets: self.task_stats.clt.recv.get_packets(),
                client_wr_bytes: self.task_stats.clt.send.get_bytes(),
                client_wr_packets: self.task_stats.clt.send.get_packets(),
                remote_rd_bytes: self.task_stats.ups.recv.get_bytes(),
                remote_rd_packets: self.task_stats.ups.recv.get_packets(),
                remote_wr_bytes: self.task_stats.ups.send.get_bytes(),
                remote_wr_packets: self.task_stats.ups.send.get_packets(),
            })
    }

    pub(crate) fn into_running<CDR, CDW>(mut self, clt_r: CDR, clt_w: HttpClientWriter<CDW>)
    where
        CDR: AsyncRead + Send + Sync + Unpin + 'static,
        CDW: AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let Some((ups_r, ups_w)) = self.ups_c.take() else {
            return;
        };

        tokio::spawn(async move {
            let e = match self.run_connected(clt_r, clt_w, ups_r, ups_w).await {
                Ok(_) => ServerTaskError::Finished,
                Err(e) => e,
            };
            if let Some(log_ctx) = self.get_log_context() {
                log_ctx.log(e);
            }
        });
    }

    async fn run_connected<CDR, CDW, UR, UW>(
        &mut self,
        clt_r: CDR,
        mut clt_w: HttpClientWriter<CDW>,
        ups_r: UR,
        ups_w: UW,
    ) -> ServerTaskResult<()>
    where
        CDR: AsyncRead + Send + Sync + Unpin + 'static,
        CDW: AsyncWrite + Send + Sync + Unpin + 'static,
        UR: UdpCopyRemoteRecv + Send + Sync + Unpin + 'static,
        UW: UdpCopyRemoteSend + Send + Sync + Unpin + 'static,
    {
        if self.ctx.server_config.flush_task_log_on_connected
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_connected();
        }

        self.task_notes.stage = ServerTaskStage::Replying;
        self.reply_upgraded(&mut clt_w).await?;

        self.task_notes.mark_relaying();
        if let Some(user_ctx) = self.task_notes.user_ctx() {
            user_ctx.foreach_req_stats(|s| {
                s.req_ready.add_http_connect();
            });
        }
        let clt_w = clt_w.into_inner();
        self.relay(clt_r, clt_w, ups_r, ups_w).await
    }

    async fn relay<CDR, CDW, UR, UW>(
        &mut self,
        clt_r: CDR,
        clt_w: CDW,
        ups_r: UR,
        ups_w: UW,
    ) -> ServerTaskResult<()>
    where
        CDR: AsyncRead + Send + Sync + Unpin + 'static,
        CDW: AsyncWrite + Send + Sync + Unpin + 'static,
        UR: UdpCopyRemoteRecv + Send + Sync + Unpin + 'static,
        UW: UdpCopyRemoteSend + Send + Sync + Unpin + 'static,
    {
        let tcp_wrapper_stats = Arc::new(HttpConnectUdpTaskServerCltWrapperStats::new(
            self.ctx.server_stats.clone(),
        ));
        let clt_r = LimitedReader::local_limited(
            clt_r,
            self.ctx.server_config.tcp_sock_speed_limit.shift_millis,
            self.ctx.server_config.tcp_sock_speed_limit.max_north,
            tcp_wrapper_stats.clone(),
        );
        let clt_w = LimitedWriter::local_limited(
            clt_w,
            self.ctx.server_config.tcp_sock_speed_limit.shift_millis,
            self.ctx.server_config.tcp_sock_speed_limit.max_south,
            tcp_wrapper_stats,
        );

        let max_packet_size = self.ctx.server_config.udp_relay.packet_size();
        let clt_r = HttpConnectUdpRecv::new(
            clt_r,
            self.ctx.server_config.udp_relay.underlying_buffer_size(),
            max_packet_size,
        );
        let clt_w = HttpConnectUdpSend::new(clt_w, max_packet_size);

        let mut udp_wrapper_stats = HttpConnectUdpTaskCltWrapperStats::new(&self.task_stats);
        if let Some(user_ctx) = self.task_notes.user_ctx() {
            let user_io_stats = user_ctx.fetch_traffic_stats(
                self.ctx.server_config.name(),
                self.ctx.server_stats.share_extra_tags(),
            );

            udp_wrapper_stats.push_user_io_stats(user_io_stats);

            let wrapper_stats = Arc::new(udp_wrapper_stats);
            let mut clt_r = LimitedUdpCopyClientRecv::unlimited(clt_r, wrapper_stats.clone());
            let mut clt_w = LimitedUdpCopyClientSend::unlimited(clt_w, wrapper_stats);

            let user = user_ctx.user();
            if let Some(limiter) = user.udp_all_upload_speed_limit() {
                clt_r.add_global_limiter(limiter.clone());
            }
            if let Some(limiter) = user.udp_all_download_speed_limit() {
                clt_w.add_global_limiter(limiter.clone());
            }
            self.run_relay(clt_r, clt_w, ups_r, ups_w).await
        } else {
            let wrapper_stats = Arc::new(udp_wrapper_stats);
            let clt_r = LimitedUdpCopyClientRecv::unlimited(clt_r, wrapper_stats.clone());
            let clt_w = LimitedUdpCopyClientSend::unlimited(clt_w, wrapper_stats);
            self.run_relay(clt_r, clt_w, ups_r, ups_w).await
        }
    }

    async fn run_relay<CR, CW, UR, UW>(
        &mut self,
        mut clt_r: CR,
        mut clt_w: CW,
        mut ups_r: UR,
        mut ups_w: UW,
    ) -> ServerTaskResult<()>
    where
        CR: UdpCopyClientRecv + Send + Sync + Unpin + 'static,
        CW: UdpCopyClientSend + Send + Sync + Unpin + 'static,
        UR: UdpCopyRemoteRecv + Send + Sync + Unpin + 'static,
        UW: UdpCopyRemoteSend + Send + Sync + Unpin + 'static,
    {
        let task_id = &self.task_notes.id;

        let mut c_to_r =
            UdpCopyClientToRemote::new(&mut clt_r, &mut ups_w, self.ctx.server_config.udp_relay);
        let mut r_to_c =
            UdpCopyRemoteToClient::new(&mut clt_w, &mut ups_r, self.ctx.server_config.udp_relay);

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut log_interval = self.ctx.get_log_interval();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;

                r = &mut c_to_r => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(UdpCopyError::RecvError(e)) => Err(e.into()),
                        Err(UdpCopyError::SendError(e)) => {
                            if let Some(logger) = ups_w.error_logger() {
                                EscapeLogForUdpConnectSendTo {
                                    task_id,
                                    upstream: Some(&self.upstream),
                                    udp_notes: &self.udp_notes,
                                }
                                .log(logger, &e);
                            }
                            Err(e.into())
                        },
                        Err(UdpCopyError::SendZero) => Err(ServerTaskError::ClosedByUpstream),
                    };
                }
                r = &mut r_to_c => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(UdpCopyError::RecvError(e)) => {
                            if let Some(logger) = ups_r.error_logger() {
                                EscapeLogForUdpConnectSendTo {
                                    task_id,
                                    upstream: Some(&self.upstream),
                                    udp_notes: &self.udp_notes,
                                }
                                .log(logger, &e);
                            }
                            Err(e.into())
                        },
                        Err(UdpCopyError::SendError(e)) => Err(e.into()),
                        Err(UdpCopyError::SendZero) => Err(ServerTaskError::ClosedByClient),
                    };
                }
                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
                n = idle_interval.tick() => {
                    if c_to_r.is_idle() && r_to_c.is_idle() {
                        idle_count += n;

                        if let Some(user_ctx) = self.task_notes.user_ctx() {
                            let user = user_ctx.user();
                            if user.is_blocked() {
                                return Err(ServerTaskError::CanceledAsUserBlocked);
                            }
                        }

                        if idle_count >= self.max_idle_count {
                            return Err(ServerTaskError::Idle(idle_interval.period(), idle_count));
                        }
                    } else {
                        idle_count = 0;

                        c_to_r.reset_active();
                        r_to_c.reset_active();
                    }

                    if let Some(user_ctx) = self.task_notes.user_ctx()
                        && user_ctx.user().is_blocked() {
                            return Err(ServerTaskError::CanceledAsUserBlocked);
                        }

                    if self.ctx.server_quit_policy.force_quit() {
                        return Err(ServerTaskError::CanceledAsServerQuit)
                    }
                }
            }
        }
    }
}
