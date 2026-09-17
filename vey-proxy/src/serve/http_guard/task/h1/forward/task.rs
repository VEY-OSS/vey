/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use futures_util::FutureExt;
use http::{HeaderMap, header};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_http::client::HttpForwardRemoteResponse;
use vey_http::server::HttpProxyClientRequest;
use vey_http::{HttpBodyReader, HttpBodyType};
use vey_icap_client::reqmod::h1::{
    H1ReqmodAdaptationError, HttpAdapterErrorResponse, HttpRequestAdapter,
    ReqmodAdaptationEndState, ReqmodAdaptationRunState, ReqmodRecvHttpResponseBody,
};
use vey_icap_client::respmod::h1::{
    HttpResponseAdapter, RespmodAdaptationEndState, RespmodAdaptationRunState,
};
use vey_io_ext::{
    GlobalLimitGroup, LimitedBufReadExt, LimitedReadExt, LimitedWriteExt, StreamCopy,
    StreamCopyError,
};
use vey_types::acl::AclAction;
use vey_types::net::{KeepAliveValue, TcpSockSpeedLimitConfig};

use super::protocol::{HttpClientReader, HttpClientWriter, HttpGuardRequest};
use super::{H1TaskContext, HttpForwardTaskCltWrapperStats, HttpForwardTaskStats};
use crate::audit::AuditContext;
use crate::config::server::ServerConfig;
use crate::escape::EgressNotes;
use crate::log::task::http_forward::TaskLogForHttpForward;
use crate::module::http_forward::{
    BoxHttpForwardConnection, BoxHttpForwardContext, BoxHttpForwardReader, BoxHttpForwardWriter,
    HttpAliveReuseNotes, HttpForwardTaskNotes, HttpProxyClientResponse,
};
use crate::module::tcp_connect::{TcpConnectError, TcpConnectTaskConf, TlsConnectTaskConf};
use crate::serve::http_guard::HttpForwardTaskAliveGuard;
use crate::serve::{
    ServerIdleChecker, ServerStats, ServerTaskError, ServerTaskForbiddenError, ServerTaskNotes,
    ServerTaskResult, ServerTaskStage,
};
use crate::site::{Site, SiteContext};
use crate::stat::types::RequestAliveKind;

pub(crate) struct HttpGuardForwardTask<'a> {
    ctx: Arc<H1TaskContext>,
    site_ctx: SiteContext,
    req: &'a HttpProxyClientRequest,
    should_close: bool,
    ups_keep_alive: KeepAliveValue,
    allow_continue: bool,
    send_error_response: bool,
    task_notes: ServerTaskNotes,
    http_notes: HttpForwardTaskNotes,
    egress_notes: EgressNotes,
    task_stats: Arc<HttpForwardTaskStats>,
    max_idle_count: usize,
    audit_task: bool,
    _alive_guard: Option<HttpForwardTaskAliveGuard>,
    alive_reuse_notes: Option<HttpAliveReuseNotes>,
    origin_session_auth: bool,
}

impl<'a> HttpGuardForwardTask<'a> {
    pub(crate) fn new(
        ctx: &Arc<H1TaskContext>,
        req: &'a HttpGuardRequest<impl AsyncRead>,
        site_ctx: SiteContext,
        task_notes: ServerTaskNotes,
        origin_session_auth: bool,
    ) -> Self {
        let uri_log_max_chars = site_ctx
            .log_uri_max_chars()
            .unwrap_or(ctx.server_config.log_uri_max_chars);
        let http_notes = HttpForwardTaskNotes::new(
            req.time_received,
            task_notes.task_created_instant(),
            req.inner.method.clone(),
            req.inner.uri.clone(),
            uri_log_max_chars,
        );
        let max_idle_count = task_notes.task_max_idle_count(ctx.server_config.task_idle_max_count);
        HttpGuardForwardTask {
            ctx: Arc::clone(ctx),
            site_ctx,
            req: &req.inner,
            should_close: !req.inner.keep_alive(),
            ups_keep_alive: KeepAliveValue::default(),
            allow_continue: req.inner.expect_100_continue(),
            send_error_response: true,
            task_notes,
            http_notes,
            egress_notes: EgressNotes::default(),
            task_stats: Arc::new(HttpForwardTaskStats::default()),
            max_idle_count,
            audit_task: false,
            _alive_guard: None,
            alive_reuse_notes: None,
            origin_session_auth,
        }
    }

    fn site(&self) -> &Site {
        self.site_ctx.site()
    }

    fn origin_tls(&self) -> bool {
        self.site().tls_client().is_some()
    }

    fn rsp_hdr_recv_timeout(&self) -> Duration {
        self.site_ctx
            .rsp_hdr_recv_timeout()
            .unwrap_or(self.ctx.server_config.timeout.recv_rsp_header)
    }

    #[inline]
    pub(crate) fn should_close(&self) -> bool {
        self.should_close
    }

    #[inline]
    pub(crate) fn origin_session_auth(&self) -> bool {
        self.origin_session_auth
    }

    fn enable_custom_header_for_local_reply(&self, rsp: &mut HttpProxyClientResponse) {
        self.ctx.apply_proxy_status_ident(rsp);
    }

    async fn reply_too_many_requests<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::too_many_requests(self.req.version);
        self.enable_custom_header_for_local_reply(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_ok() {
            self.http_notes.rsp_status = rsp.status();
        }
        self.should_close = true;
    }

    async fn reply_forbidden<W>(&mut self, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::forbidden(self.req.version);
        self.enable_custom_header_for_local_reply(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_ok() {
            self.http_notes.rsp_status = rsp.status();
        }
        self.should_close = true;
    }

    async fn reply_connect_err<W>(&mut self, e: &TcpConnectError, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::from_tcp_connect_error(
            e,
            self.req.version,
            self.should_close || self.req.body_type().is_some(),
        );

        self.enable_custom_header_for_local_reply(&mut rsp);

        if rsp.should_close() {
            self.should_close = true;
        }

        if rsp.reply_err_to_request(clt_w).await.is_err() {
            self.should_close = true;
        } else {
            self.http_notes.rsp_status = rsp.status();
        }
    }

    async fn reply_task_err<W>(&mut self, e: &ServerTaskError, clt_w: &mut W)
    where
        W: AsyncWrite + Unpin,
    {
        let body_pending = self.req.body_type().is_some();
        let rsp = HttpProxyClientResponse::from_task_err(
            e,
            self.req.version,
            self.should_close || body_pending,
        );

        if let Some(mut rsp) = rsp {
            self.enable_custom_header_for_local_reply(&mut rsp);

            if rsp.should_close() {
                self.should_close = true;
            }

            if rsp.reply_err_to_request(clt_w).await.is_err() {
                self.should_close = true;
            } else {
                self.http_notes.rsp_status = rsp.status();
            }
        } else if body_pending {
            self.should_close = true;
        }
    }

    fn get_log_context(&self) -> Option<TaskLogForHttpForward<'_>> {
        let Some(logger) = &self.ctx.task_logger else {
            return None;
        };

        let http_user_agent = self
            .req
            .end_to_end_headers
            .get(header::USER_AGENT)
            .map(|v| v.to_str());
        Some(TaskLogForHttpForward {
            logger,
            upstream: self.site().upstream(),
            task_notes: &self.task_notes,
            http_notes: &self.http_notes,
            http_user_agent,
            egress_notes: &self.egress_notes,
            client_rd_bytes: self.task_stats.clt.read.get_bytes(),
            client_wr_bytes: self.task_stats.clt.write.get_bytes(),
            remote_rd_bytes: self.task_stats.ups.read.get_bytes(),
            remote_wr_bytes: self.task_stats.ups.write.get_bytes(),
        })
    }

    pub(crate) async fn run<CDR, CDW>(
        &mut self,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
        fwd_ctx: &mut BoxHttpForwardContext,
    ) where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        self.pre_start();
        let e = match self.run_forward(clt_r, clt_w, fwd_ctx).await {
            Ok(()) => ServerTaskError::Finished,
            Err(e) => e,
        };
        if let Some(log_ctx) = self.get_log_context() {
            log_ctx.log(&e);
        }
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_forward_task());

        self.task_notes
            .hold_req_alive(RequestAliveKind::HttpForward { is_https: false });

        if self.ctx.server_config.flush_task_log_on_created
            && let Some(log_ctx) = self.get_log_context()
        {
            log_ctx.log_created();
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

    async fn handle_user_ua_acl_action<W>(
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
                ServerTaskForbiddenError::UaBlocked,
            ))
        } else {
            Ok(())
        }
    }

    fn clt_speed_limit(&self) -> Option<TcpSockSpeedLimitConfig> {
        let server = self.ctx.server_config.tcp_sock_speed_limit;
        let limit = self
            .site_ctx
            .tcp_sock_speed_limit()
            .shrink_as_smaller(&server);
        if limit.eq(&server) { None } else { Some(limit) }
    }

    fn setup_clt_limit_and_stats<CDR, CDW>(
        &mut self,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
    ) where
        CDR: AsyncRead + Unpin,
        CDW: AsyncWrite + Unpin,
    {
        let origin_header_size = self.req.origin_header_size() as u64;
        self.task_stats.clt.read.add_bytes(origin_header_size);

        let mut wrapper_stats =
            HttpForwardTaskCltWrapperStats::new(&self.ctx.server_stats, &self.task_stats);

        let user_io_stats = self.task_notes.fetch_traffic_stats(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );
        for s in &user_io_stats {
            s.io.http_forward.add_in_bytes(origin_header_size);
        }
        wrapper_stats.push_user_io_stats(user_io_stats);

        let (clt_r_stats, clt_w_stats) = wrapper_stats.split();
        let limit_config = self.clt_speed_limit();

        clt_w.retain_global_limiter_by_group(GlobalLimitGroup::Server);
        if let Some(br) = clt_r {
            br.reset_buffer_stats(clt_r_stats);
            clt_w.reset_stats(clt_w_stats);
            if let Some(limit_config) = &limit_config {
                br.reset_local_limit(limit_config.shift_millis, limit_config.max_north);
                clt_w.reset_local_limit(limit_config.shift_millis, limit_config.max_south);
            }
            if let Some(user) = self.task_notes.tenant_user() {
                if let Some(limiter) = user.tcp_all_upload_speed_limit() {
                    limiter.try_consume(origin_header_size);
                    br.add_global_limiter(limiter.clone());
                }
                if let Some(limiter) = user.tcp_all_download_speed_limit() {
                    clt_w.add_global_limiter(limiter.clone());
                }
            }
        } else {
            clt_w.reset_stats(clt_w_stats);
            if let Some(limit_config) = &limit_config {
                clt_w.reset_local_limit(limit_config.shift_millis, limit_config.max_south);
            }
            if let Some(user) = self.task_notes.tenant_user() {
                if let Some(limiter) = user.tcp_all_upload_speed_limit() {
                    limiter.try_consume(origin_header_size);
                }
                if let Some(limiter) = user.tcp_all_download_speed_limit() {
                    clt_w.add_global_limiter(limiter.clone());
                }
            }
        }
    }

    async fn run_forward<CDR, CDW>(
        &mut self,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
        fwd_ctx: &mut BoxHttpForwardContext,
    ) -> ServerTaskResult<()>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        if self.task_notes.check_layered_rate_limit().is_err() {
            self.reply_too_many_requests(clt_w).await;
            return Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::RateLimited,
            ));
        }

        if self.task_notes.acquire_site_request_semaphores().is_err() {
            self.reply_too_many_requests(clt_w).await;
            return Err(ServerTaskError::ForbiddenByRule(
                ServerTaskForbiddenError::FullyLoaded,
            ));
        }

        let tenant = self.task_notes.tenant_ctx().cloned();
        let mut audit_task = false;
        let tcp_client_misc_opts = if let Some(tenant) = &tenant {
            let action = tenant.check_upstream(self.site().upstream());
            self.handle_user_upstream_acl_action(action, clt_w).await?;

            if let Some(action) = tenant.check_http_user_agent(
                self.req
                    .end_to_end_headers
                    .get_all(header::USER_AGENT)
                    .iter()
                    .map(|v| v.to_str()),
            ) {
                self.handle_user_ua_acl_action(action, clt_w).await?;
            }

            if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
                audit_task = tenant
                    .user()
                    .audit()
                    .do_task_audit()
                    .unwrap_or_else(|| audit_handle.do_task_audit());
            }

            tenant
                .user_config()
                .tcp_client_misc_opts(&self.ctx.server_config.tcp_misc_opts)
        } else {
            if let Some(audit_handle) = self.ctx.audit_handle.as_ref() {
                audit_task = audit_handle.do_task_audit();
            }
            Cow::Borrowed(&self.ctx.server_config.tcp_misc_opts)
        };
        self.audit_task = audit_task;

        // set client side socket options
        self.ctx
            .cc_info
            .tcp_sock_set_raw_opts(&tcp_client_misc_opts, true)
            .map_err(|_| {
                ServerTaskError::InternalServerError("failed to set client socket options")
            })?;

        self.setup_clt_limit_and_stats(clt_r, clt_w);

        let keepalive = self.site().h1_keepalive_config();
        if keepalive.is_enabled()
            && let Some(mut connection) = self
                .take_alive_origin_connection(fwd_ctx, keepalive.idle_expire())
                .await
        {
            self.task_notes.stage = ServerTaskStage::Connected;
            self.http_notes.reused_connection = true;
            self.http_notes.retry_new_connection = false;
            self.task_notes
                .foreach_req_stats(|s| s.req_reuse.add_http_forward(false));

            if self.ctx.server_config.flush_task_log_on_connected
                && let Some(log_ctx) = self.get_log_context()
            {
                log_ctx.log_connected();
            }

            connection
                .0
                .prepare_new(&self.task_notes, self.site().upstream());
            self.mark_relaying();

            let r = self
                .run_with_connection(fwd_ctx, clt_r, clt_w, connection, audit_task)
                .await;
            match r {
                Ok(ups_s) => {
                    self.save_or_close(fwd_ctx, clt_w, ups_s).await;
                    return Ok(());
                }
                Err(e) => {
                    if self.http_notes.retry_new_connection {
                        if let Some(log_ctx) = self.get_log_context() {
                            log_ctx.log(&e);
                        }
                        self.task_stats.ups.reset();
                        // continue to make new connection
                        self.task_notes
                            .foreach_req_stats(|s| s.req_renew.add_http_forward(false));
                    } else {
                        self.should_close = true;
                        if self.send_error_response {
                            self.reply_task_err(&e, clt_w).await;
                        }
                        return Err(e);
                    }
                }
            }
        }

        let connection = self.get_new_connection(fwd_ctx, clt_w).await?;
        match self
            .run_with_connection(fwd_ctx, clt_r, clt_w, connection, audit_task)
            .await
        {
            Ok(ups_s) => {
                self.save_or_close(fwd_ctx, clt_w, ups_s).await;
                Ok(())
            }
            Err(e) => {
                self.should_close = true;
                if self.send_error_response {
                    self.reply_task_err(&e, clt_w).await;
                }
                Err(e)
            }
        }
    }

    async fn take_alive_origin_connection(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        idle_expire: Duration,
    ) -> Option<BoxHttpForwardConnection> {
        if self.origin_session_auth {
            self.take_alive_from_fwd_ctx(fwd_ctx, idle_expire).await
        } else if let Some(pool) = self.site_ctx.site().http1_pool() {
            let (connection, reuse_notes, egress_notes) = pool
                .get(
                    self.task_notes.worker_id(),
                    self.ctx.escaper.name(),
                    idle_expire,
                )
                .await?;

            self.egress_notes = egress_notes;
            let connection = reuse_notes.escaper.prepare_reused_http_forward_connection(
                connection,
                &self.task_notes,
                self.task_stats.clone(),
                self.origin_tls(),
            );
            self.alive_reuse_notes = Some(reuse_notes);
            Some(connection)
        } else {
            self.take_alive_from_fwd_ctx(fwd_ctx, idle_expire).await
        }
    }

    async fn take_alive_from_fwd_ctx(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        idle_expire: Duration,
    ) -> Option<BoxHttpForwardConnection> {
        let (connection, reuse_notes) = fwd_ctx
            .get_prepared_alive_connection(
                &self.task_notes,
                self.task_stats.clone(),
                idle_expire,
                self.origin_tls(),
            )
            .await?;
        self.alive_reuse_notes = Some(reuse_notes);
        fwd_ctx.fetch_egress_notes(&mut self.egress_notes);
        Some(connection)
    }

    async fn save_or_close<CDW>(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        clt_w: &mut HttpClientWriter<CDW>,
        ups_s: Option<BoxHttpForwardConnection>,
    ) where
        CDW: AsyncWrite + Unpin,
    {
        if self.should_close {
            if let Some(mut connection) = ups_s {
                let _ = connection.0.shutdown().await;
            }
            let _ = clt_w.shutdown().await;
        } else if let Some(connection) = ups_s {
            self.save_alive_origin_connection(fwd_ctx, connection);
        }
    }

    fn save_alive_origin_connection(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        connection: BoxHttpForwardConnection,
    ) {
        if self.origin_session_auth {
            fwd_ctx.save_alive_connection(connection, self.ups_keep_alive);
            return;
        }
        if !self.site().h1_keepalive_config().is_enabled() {
            return;
        }
        if let Some(pool) = self.site_ctx.site().http1_pool() {
            let Some(reuse_notes) = self.alive_reuse_notes.take() else {
                return;
            };
            pool.save(
                self.task_notes.worker_id(),
                self.ctx.escaper.name().clone(),
                connection,
                reuse_notes,
                self.egress_notes.clone(),
            );
        } else {
            fwd_ctx.save_alive_connection(connection, self.ups_keep_alive);
        }
    }

    async fn get_new_connection<CDW>(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        clt_w: &mut HttpClientWriter<CDW>,
    ) -> ServerTaskResult<BoxHttpForwardConnection>
    where
        CDW: AsyncWrite + Unpin,
    {
        self.task_notes.stage = ServerTaskStage::Connecting;
        self.http_notes.reused_connection = false;
        self.alive_reuse_notes = None;

        match self.make_new_connection(fwd_ctx).await {
            Ok((mut connection, reuse_notes)) => {
                self.alive_reuse_notes = Some(reuse_notes);
                self.task_notes.stage = ServerTaskStage::Connected;
                fwd_ctx.fetch_egress_notes(&mut self.egress_notes);

                if self.ctx.server_config.flush_task_log_on_connected
                    && let Some(log_ctx) = self.get_log_context()
                {
                    log_ctx.log_connected();
                }

                connection
                    .0
                    .prepare_new(&self.task_notes, self.site().upstream());
                self.mark_relaying();
                Ok(connection)
            }
            Err(e) => {
                fwd_ctx.fetch_egress_notes(&mut self.egress_notes);
                self.should_close = true;
                self.reply_connect_err(&e, clt_w).await;
                Err(e.into())
            }
        }
    }

    async fn make_new_connection(
        &self,
        fwd_ctx: &mut BoxHttpForwardContext,
    ) -> Result<(BoxHttpForwardConnection, HttpAliveReuseNotes), TcpConnectError> {
        let mut audit_ctx = AuditContext::new(self.ctx.audit_handle.clone());
        if let Some(tls_client) = self.site().tls_client() {
            let task_conf = TlsConnectTaskConf {
                tcp: TcpConnectTaskConf {
                    upstream: self.site().upstream(),
                },
                tls_config: tls_client,
                tls_name: self.site().tls_name(),
                alpn_protocols: None,
            };
            fwd_ctx
                .new_prepared_https_connection(
                    &task_conf,
                    &self.task_notes,
                    self.task_stats.clone(),
                    &mut audit_ctx,
                )
                .await
        } else {
            let task_conf = TcpConnectTaskConf {
                upstream: self.site().upstream(),
            };
            fwd_ctx
                .new_prepared_http_connection(
                    &task_conf,
                    &self.task_notes,
                    self.task_stats.clone(),
                    &mut audit_ctx,
                )
                .await
        }
    }

    fn mark_relaying(&mut self) {
        self.task_notes.mark_relaying();
        self.task_notes
            .foreach_req_stats(|s| s.req_ready.add_http_forward(false));
    }

    async fn run_with_connection<CDR, CDW>(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
        audit_task: bool,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        if self.http_notes.reused_connection
            && let Some(r) = ups_c.1.fill_wait_data().now_or_never()
        {
            self.http_notes.retry_new_connection = true;
            return match r {
                Ok(true) => Err(ServerTaskError::UpstreamAppError(anyhow!(
                    "unexpected data found when polling IDLE connection"
                ))),
                Ok(false) => Err(ServerTaskError::ClosedByUpstream),
                Err(e) => Err(ServerTaskError::UpstreamReadFailed(e)),
            };
        }

        if audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(reqmod) = audit_handle.icap_reqmod_client()
        {
            match reqmod
                .h1_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    true,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state =
                        ReqmodAdaptationRunState::new(self.task_notes.task_created_instant());
                    adapter.set_client_addr(self.ctx.client_addr());
                    if let Some(name) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(name.clone());
                    }
                    if let Some(name) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(name.clone());
                    }
                    let r = self
                        .run_with_adaptation(clt_r, clt_w, ups_c, adapter, &mut adaptation_state)
                        .await;
                    if let Some(dur) = adaptation_state.dur_ups_send_header {
                        self.http_notes.dur_req_send_hdr = dur;
                    }
                    if let Some(dur) = adaptation_state.dur_ups_send_all {
                        self.http_notes.dur_req_send_all = dur;
                    }
                    self.http_notes.clt_req_body_size = adaptation_state.clt_req_body_size;
                    self.http_notes.ups_req_body_size = adaptation_state.ups_req_body_size;
                    return r;
                }
                Err(e) => {
                    self.http_notes.retry_new_connection = true;
                    if !reqmod.bypass() {
                        return Err(ServerTaskError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.run_without_adaptation(fwd_ctx, clt_r, clt_w, ups_c)
            .await
    }

    async fn run_with_adaptation<CDR, CDW>(
        &mut self,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
        icap_adapter: HttpRequestAdapter<ServerIdleChecker>,
        adaptation_state: &mut ReqmodAdaptationRunState,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        use crate::module::http_forward::HttpForwardWriterForAdaptation;

        let ups_w = &mut ups_c.0;
        let ups_r = &mut ups_c.1;

        let mut ups_w_adaptation = HttpForwardWriterForAdaptation { inner: ups_w };
        let mut adaptation_fut = icap_adapter
            .xfer(
                adaptation_state,
                self.req,
                clt_r.as_mut(),
                &mut ups_w_adaptation,
            )
            .boxed();

        let mut log_interval = self.ctx.get_log_interval();

        let clt_read_size = self.task_stats.clt.read.get_bytes();
        let mut rsp_header: Option<HttpForwardRemoteResponse> = None;
        loop {
            tokio::select! {
                biased;

                r = ups_r.fill_wait_data() => {
                    match r {
                        Ok(true) => {
                            let hdr = self.recv_response_header(ups_r).await?;
                            if let Some(final_hdr) = self.check_out_final_response(hdr, clt_w).await? {
                                rsp_header = Some(final_hdr);
                                break;
                            }
                        }
                        Ok(false) =>  {
                            if self.task_stats.clt.read.get_bytes() == clt_read_size {
                                self.http_notes.retry_new_connection = true;
                            }
                            return Err(ServerTaskError::ClosedByUpstream);
                        },
                        Err(e) => {
                            if self.task_stats.clt.read.get_bytes() == clt_read_size {
                                self.http_notes.retry_new_connection = true;
                            }
                            return Err(ServerTaskError::UpstreamReadFailed(e));
                        },
                    }
                }
                r = &mut adaptation_fut => {
                    match r {
                        Ok(ReqmodAdaptationEndState::OriginalTransferred) => {
                            break;
                        }
                        Ok(ReqmodAdaptationEndState::AdaptedTransferred(_r)) => {
                            break;
                        }
                        Ok(ReqmodAdaptationEndState::HttpErrResponse(rsp, rsp_recv_body)) => {
                            self.send_adaptation_error_response(clt_w, rsp, rsp_recv_body).await?;
                            return Ok(None);
                        }
                        Err(e) => {
                            if self.task_stats.clt.read.get_bytes() == clt_read_size {
                                self.http_notes.retry_new_connection = matches!(
                                    e,
                                    H1ReqmodAdaptationError::IcapServerConnectionClosed | H1ReqmodAdaptationError::IcapServerReadFailed(_)
                                );
                            }
                            return Err(e.into());
                        }
                    }
                }
                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
            }
        }
        drop(adaptation_fut);

        let mut close_remote = false;
        let mut rsp_header = match rsp_header {
            Some(header) => {
                if !adaptation_state.clt_read_finished {
                    self.should_close = true;
                }
                if !adaptation_state.ups_write_finished {
                    close_remote = true;
                }
                header
            }
            None => {
                match tokio::time::timeout(
                    self.rsp_hdr_recv_timeout(),
                    self.recv_final_response_header(ups_r, clt_w),
                )
                .await
                {
                    Ok(Ok(rsp_header)) => rsp_header,
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(ServerTaskError::UpstreamAppTimeout(
                            "timeout to receive response header",
                        ));
                    }
                }
            }
        };
        self.http_notes.mark_rsp_recv_hdr();

        self.send_response(
            clt_w,
            ups_r,
            &mut rsp_header,
            adaptation_state.take_respond_shared_headers(),
        )
        .await?;

        self.task_notes.stage = ServerTaskStage::Finished;
        if close_remote {
            let _ = ups_w.shutdown().await;
            Ok(None)
        } else {
            Ok(Some(ups_c))
        }
    }

    async fn send_adaptation_error_response<W>(
        &mut self,
        clt_w: &mut W,
        mut rsp: HttpAdapterErrorResponse,
        rsp_recv_body: Option<ReqmodRecvHttpResponseBody>,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        self.should_close = true;

        self.ctx
            .set_custom_header_for_adaptation_error_reply(&self.egress_notes, &mut rsp);

        let buf = rsp.serialize(self.should_close);
        self.send_error_response = false;
        clt_w
            .write_all(buf.as_ref())
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        self.http_notes.rsp_status = rsp.status.as_u16();

        if let Some(mut recv_body) = rsp_recv_body {
            let mut body_reader = recv_body.body_reader();
            let mut copy_to_clt =
                StreamCopy::new(&mut body_reader, clt_w, &self.ctx.server_config.tcp_copy);
            (&mut copy_to_clt).await.map_err(|e| match e {
                StreamCopyError::ReadFailed(e) => ServerTaskError::InternalAdapterError(anyhow!(
                    "read http error response from adapter failed: {e:?}"
                )),
                StreamCopyError::WriteFailed(e) => ServerTaskError::ClientTcpWriteFailed(e),
            })?;
            self.http_notes.clt_rsp_body_size = Some(copy_to_clt.reader().body_size());
            recv_body.save_connection().await;
        } else {
            self.http_notes.clt_rsp_body_size = Some(0);
            clt_w
                .flush()
                .await
                .map_err(ServerTaskError::ClientTcpWriteFailed)?;
        }

        Ok(())
    }

    async fn run_without_adaptation<CDR, CDW>(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        CDR: AsyncRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        let Some(body_type) = self.req.body_type() else {
            return self.run_without_body(clt_w, ups_c).await;
        };
        let Some(clt_r) = clt_r else {
            return Err(ServerTaskError::InternalServerError(
                "http body is expected but no body reader supplied",
            ));
        };

        let mut clt_body_reader = HttpBodyReader::new(
            clt_r,
            body_type,
            self.ctx.server_config.h1.body_line_max_len,
        );

        if self.req.end_to_end_headers.contains_key(header::EXPECT) {
            return self
                .run_with_body(None, &mut clt_body_reader, clt_w, ups_c)
                .await;
        }

        // SAFETY: only `[..nr]` is kept after read_all_now fills it.
        let n = self.ctx.server_config.tcp_copy.buffer_size();
        let mut fast_read_buf =
            Vec::from(unsafe { Box::<[u8]>::new_uninit_slice(n).assume_init() });
        let nr = clt_body_reader
            .read_all_now(&mut fast_read_buf)
            .await
            .map_err(ServerTaskError::ClientTcpReadFailed)?
            .ok_or(ServerTaskError::ClosedByClient)?;
        if nr == 0 {
            drop(fast_read_buf);
            return self
                .run_with_body(None, &mut clt_body_reader, clt_w, ups_c)
                .await;
        }
        fast_read_buf.truncate(nr);

        if clt_body_reader.finished() {
            self.http_notes.clt_req_body_size = Some(clt_body_reader.body_size());
            return self
                .run_with_all_body(fwd_ctx, fast_read_buf, clt_w, ups_c)
                .await;
        }

        loop {
            match self
                .run_with_body(
                    Some(fast_read_buf.clone()),
                    &mut clt_body_reader,
                    clt_w,
                    ups_c,
                )
                .await
            {
                Ok(r) => return Ok(r),
                Err(e) => {
                    if self.http_notes.reused_connection && self.http_notes.retry_new_connection {
                        if let Some(log_ctx) = self.get_log_context() {
                            log_ctx.log(&e);
                        }
                        self.task_stats.ups.reset();
                        ups_c = self.get_new_connection(fwd_ctx, clt_w).await?;
                    } else {
                        self.http_notes.retry_new_connection = false;
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn run_without_body<CDW>(
        &mut self,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        CDW: AsyncWrite + Send + Unpin,
    {
        let ups_w = &mut ups_c.0;
        let ups_r = &mut ups_c.1;

        self.http_notes.retry_new_connection = true;
        ups_w
            .send_request_header(self.req, None)
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        ups_w
            .flush()
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        self.http_notes.mark_req_send_hdr();
        self.http_notes.mark_req_no_body();

        let mut rsp_header = match tokio::time::timeout(
            self.rsp_hdr_recv_timeout(),
            self.recv_final_response_header(ups_r, clt_w),
        )
        .await
        {
            Ok(Ok(rsp_header)) => {
                self.http_notes.retry_new_connection = false;
                rsp_header
            }
            Ok(Err(e)) => {
                if self.task_stats.ups.read.get_bytes() == 0 {
                    self.http_notes.retry_new_connection = matches!(
                        e,
                        ServerTaskError::ClosedByUpstream | ServerTaskError::UpstreamReadFailed(_)
                    );
                } else {
                    self.http_notes.retry_new_connection = false;
                }
                return Err(e);
            }
            Err(_) => {
                self.http_notes.retry_new_connection = false;
                return Err(ServerTaskError::UpstreamAppTimeout(
                    "timeout to receive response header",
                ));
            }
        };
        self.http_notes.mark_rsp_recv_hdr();

        self.update_response_header(&mut rsp_header);
        self.send_response(clt_w, ups_r, &mut rsp_header, None)
            .await?;

        self.task_notes.stage = ServerTaskStage::Finished;
        Ok(Some(ups_c))
    }

    async fn send_full_req_and_recv_rsp<CDW>(
        &mut self,
        body: &[u8],
        ups_r: &mut BoxHttpForwardReader,
        ups_w: &mut BoxHttpForwardWriter,
        clt_w: &mut HttpClientWriter<CDW>,
    ) -> ServerTaskResult<HttpForwardRemoteResponse>
    where
        CDW: AsyncWrite + Unpin,
    {
        self.http_notes.retry_new_connection = true;

        ups_w
            .send_request_header(self.req, Some(body))
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        ups_w
            .flush()
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        self.http_notes.mark_req_send_hdr();
        self.http_notes.mark_req_send_all();
        self.http_notes.ups_req_body_size = Some(body.len() as u64);

        match tokio::time::timeout(
            self.rsp_hdr_recv_timeout(),
            self.recv_final_response_header(ups_r, clt_w),
        )
        .await
        {
            Ok(Ok(rsp_header)) => {
                self.http_notes.retry_new_connection = false;
                Ok(rsp_header)
            }
            Ok(Err(e)) => {
                if self.task_stats.ups.read.get_bytes() == 0 {
                    self.http_notes.retry_new_connection = matches!(
                        e,
                        ServerTaskError::ClosedByUpstream | ServerTaskError::UpstreamReadFailed(_)
                    );
                } else {
                    self.http_notes.retry_new_connection = false;
                }
                Err(e)
            }
            Err(_) => {
                self.http_notes.retry_new_connection = false;
                Err(ServerTaskError::UpstreamAppTimeout(
                    "timeout to receive response header",
                ))
            }
        }
    }

    async fn run_with_all_body<CDW>(
        &mut self,
        fwd_ctx: &mut BoxHttpForwardContext,
        body: Vec<u8>,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        CDW: AsyncWrite + Send + Unpin,
    {
        loop {
            let ups_w = &mut ups_c.0;
            let ups_r = &mut ups_c.1;

            let mut rsp_header = match self
                .send_full_req_and_recv_rsp(body.as_slice(), ups_r, ups_w, clt_w)
                .await
            {
                Ok(rsp_header) => rsp_header,
                Err(e) => {
                    if self.http_notes.reused_connection && self.http_notes.retry_new_connection {
                        if let Some(log_ctx) = self.get_log_context() {
                            log_ctx.log(&e);
                        }
                        self.task_stats.ups.reset();
                        ups_c = self.get_new_connection(fwd_ctx, clt_w).await?;
                        continue;
                    } else {
                        self.http_notes.retry_new_connection = false;
                        return Err(e);
                    }
                }
            };

            self.http_notes.mark_rsp_recv_hdr();
            self.update_response_header(&mut rsp_header);

            self.send_response(clt_w, ups_r, &mut rsp_header, None)
                .await?;

            self.task_notes.stage = ServerTaskStage::Finished;
            return Ok(Some(ups_c));
        }
    }

    async fn run_with_body<R, CDW>(
        &mut self,
        fast_read_buf: Option<Vec<u8>>,
        clt_body_reader: &mut HttpBodyReader<'_, R>,
        clt_w: &mut HttpClientWriter<CDW>,
        mut ups_c: BoxHttpForwardConnection,
    ) -> ServerTaskResult<Option<BoxHttpForwardConnection>>
    where
        R: AsyncBufRead + Send + Unpin,
        CDW: AsyncWrite + Send + Unpin,
    {
        let ups_w = &mut ups_c.0;
        let ups_r = &mut ups_c.1;

        self.http_notes.retry_new_connection = true;
        ups_w
            .send_request_header(self.req, None)
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        ups_w
            .flush()
            .await
            .map_err(ServerTaskError::UpstreamWriteFailed)?;
        self.http_notes.mark_req_send_hdr();
        self.http_notes.retry_new_connection = false;

        let mut clt_to_ups = match fast_read_buf {
            Some(buf) => StreamCopy::with_data(
                clt_body_reader,
                ups_w,
                &self.ctx.server_config.tcp_copy,
                buf,
            ),
            None => StreamCopy::new(clt_body_reader, ups_w, &self.ctx.server_config.tcp_copy),
        };

        let mut rsp_header: Option<HttpForwardRemoteResponse> = None;

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut log_interval = self.ctx.get_log_interval();
        let mut idle_count = 0;

        macro_rules! record_progress {
            () => {{
                let read = clt_to_ups.reader().body_size();
                // a chunked body is copied as on-wire bytes, so the size sent
                // upstream is a lower bound: the payload read, less everything
                // still buffered, as all of it could be payload
                let written = if clt_to_ups.reader().is_chunked() {
                    read.saturating_sub(clt_to_ups.cached_data_size())
                } else {
                    clt_to_ups.copied_size()
                };
                self.http_notes.record_h1_req_body_progress(read, written)
            }};
        }
        loop {
            tokio::select! {
                biased;

                r = ups_r.fill_wait_data() => {
                    match r {
                        Ok(true) => {
                            // we got some data from upstream
                            let hdr = self.recv_response_header(ups_r).await?;
                            if let Some(final_hdr) = self.check_out_final_response(hdr, clt_w).await? {
                                rsp_header = Some(final_hdr);
                                break;
                            }
                        }
                        Ok(false) => {
                             if clt_to_ups.read_size() == 0 {
                                self.http_notes.retry_new_connection = true;
                            }
                            record_progress!();
                            return Err(ServerTaskError::ClosedByUpstream);
                        },
                        Err(e) => {
                            if clt_to_ups.read_size() == 0 {
                                self.http_notes.retry_new_connection = true;
                            }
                            record_progress!();
                            return Err(ServerTaskError::UpstreamReadFailed(e));
                        },
                    }
                }
                r = &mut clt_to_ups => {
                    match r {
                        Ok(_) => {
                            self.http_notes.mark_req_send_all();
                            let n = clt_to_ups.reader().body_size();
                            self.http_notes.clt_req_body_size = Some(n);
                            self.http_notes.ups_req_body_size = Some(n);
                            break;
                        }
                        Err(e) => {
                            record_progress!();
                            return Err(match e {
                                StreamCopyError::ReadFailed(e) => ServerTaskError::ClientTcpReadFailed(e),
                                StreamCopyError::WriteFailed(e) => ServerTaskError::UpstreamWriteFailed(e),
                            });
                        }
                    }
                }
                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
                n = idle_interval.tick() => {
                    if clt_to_ups.is_idle() {
                        idle_count += n;

                        if idle_count >= self.max_idle_count {
                            record_progress!();
                            return if clt_to_ups.no_cached_data() {
                                Err(ServerTaskError::ClientAppTimeout("idle while reading request body"))
                            } else {
                                Err(ServerTaskError::UpstreamAppTimeout("idle while sending request body"))
                            };
                        }
                    } else {
                        idle_count = 0;

                        clt_to_ups.reset_active();
                    }

                    if self.ctx.server_quit_policy.force_quit() {
                        record_progress!();
                        return Err(ServerTaskError::CanceledAsServerQuit)
                    }
                }
            };
        }
        drop(idle_interval);

        let mut close_remote = false;
        let copy_done = clt_to_ups.finished();
        let mut rsp_header = match rsp_header {
            Some(header) => {
                if !clt_body_reader.finished() {
                    // not all client data read in, drop the client connection
                    self.should_close = true;
                }
                if !copy_done {
                    // not all client data sent out, only drop the remote connection
                    close_remote = true;
                }
                // if not all data sent to remote, the remote response should be `close`,
                // and the remote connection will close if remote has set `close`
                header
            }
            None => {
                match tokio::time::timeout(
                    self.rsp_hdr_recv_timeout(),
                    self.recv_final_response_header(ups_r, clt_w),
                )
                .await
                {
                    Ok(Ok(rsp_header)) => rsp_header,
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(ServerTaskError::UpstreamAppTimeout(
                            "timeout to receive response header",
                        ));
                    }
                }
            }
        };
        self.http_notes.mark_rsp_recv_hdr();

        self.update_response_header(&mut rsp_header);
        self.send_response(clt_w, ups_r, &mut rsp_header, None)
            .await?;

        self.task_notes.stage = ServerTaskStage::Finished;
        if close_remote {
            let _ = ups_w.shutdown().await;
            Ok(None)
        } else {
            Ok(Some(ups_c))
        }
    }

    async fn recv_final_response_header<W>(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
        clt_w: &mut W,
    ) -> ServerTaskResult<HttpForwardRemoteResponse>
    where
        W: AsyncWrite + Unpin,
    {
        loop {
            let hdr = self.recv_response_header(ups_r).await?;
            if let Some(final_hdr) = self.check_out_final_response(hdr, clt_w).await? {
                return Ok(final_hdr);
            }
        }
    }

    async fn check_out_final_response<W>(
        &mut self,
        hdr: HttpForwardRemoteResponse,
        clt_w: &mut W,
    ) -> ServerTaskResult<Option<HttpForwardRemoteResponse>>
    where
        W: AsyncWrite + Unpin,
    {
        match hdr.code {
            100 => {
                // HTTP CONTINUE
                if self.allow_continue {
                    self.send_response_header(clt_w, &hdr).await?;
                    self.allow_continue = false;
                } else {
                    return Err(ServerTaskError::invalid_upstream_100_continue_response());
                }
            }
            103 => {
                // HTTP Early Hints
                self.send_response_header(clt_w, &hdr).await?;
            }
            _ => {
                return Ok(Some(hdr));
            }
        }
        Ok(None)
    }

    async fn recv_response_header(
        &mut self,
        ups_r: &mut BoxHttpForwardReader,
    ) -> ServerTaskResult<HttpForwardRemoteResponse> {
        ups_r
            .recv_response_header(
                &self.req.method,
                self.req.keep_alive(),
                self.ctx.server_config.rsp_hdr_max_size,
                &mut self.http_notes,
            )
            .await
            .map_err(|e| e.into())
    }

    async fn send_response<R, W>(
        &mut self,
        clt_w: &mut W,
        ups_r: &mut R,
        rsp_header: &mut HttpForwardRemoteResponse,
        adaptation_respond_shared_headers: Option<HeaderMap>,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Send + Unpin,
        W: AsyncWrite + Send + Unpin,
    {
        if self.should_close {
            rsp_header.set_no_keep_alive();
        }
        if !rsp_header.keep_alive() {
            self.should_close = true;
            self.ups_keep_alive = KeepAliveValue::default();
        } else {
            self.ups_keep_alive = rsp_header.keep_alive_header();
            if let Some(notes) = &mut self.alive_reuse_notes {
                notes.overlay_keep_alive(self.ups_keep_alive);
            }
        }
        self.http_notes.origin_status = rsp_header.code;
        self.http_notes.rsp_status = 0;

        if self.audit_task
            && let Some(audit_handle) = self.ctx.audit_handle.as_ref()
            && let Some(respmod) = audit_handle.icap_respmod_client()
        {
            match respmod
                .h1_adapter(
                    self.ctx.server_config.tcp_copy,
                    self.ctx.server_config.h1.body_line_max_len,
                    self.ctx.idle_checker(&self.task_notes),
                )
                .await
            {
                Ok(mut adapter) => {
                    let mut adaptation_state = RespmodAdaptationRunState::new(
                        self.task_notes.task_created_instant(),
                        self.http_notes.dur_rsp_recv_hdr,
                    );
                    adapter.set_client_addr(self.ctx.client_addr());
                    if let Some(name) = self.task_notes.raw_user_name() {
                        adapter.set_client_username(name.clone());
                    }
                    if let Some(name) = self.task_notes.tenant_user_name() {
                        adapter.set_tenant_username(name.clone());
                    }
                    adapter.set_respond_shared_headers(adaptation_respond_shared_headers);
                    let r = self
                        .send_response_with_adaptation(
                            clt_w,
                            ups_r,
                            rsp_header,
                            adapter,
                            &mut adaptation_state,
                        )
                        .await;
                    if !adaptation_state.clt_write_finished || !adaptation_state.ups_read_finished {
                        self.should_close = true;
                    }
                    if let Some(dur) = adaptation_state.dur_ups_recv_all {
                        self.http_notes.dur_rsp_recv_all = dur;
                    }
                    self.http_notes.ups_rsp_body_size = adaptation_state.ups_rsp_body_size;
                    self.http_notes.clt_rsp_body_size = adaptation_state.clt_rsp_body_size;
                    self.send_error_response = !adaptation_state.clt_write_started;
                    return r;
                }
                Err(e) => {
                    if !respmod.bypass() {
                        return Err(ServerTaskError::InternalAdapterError(e));
                    }
                }
            }
        }

        self.send_response_without_adaptation(clt_w, ups_r, rsp_header)
            .await
    }

    async fn send_response_with_adaptation<R, W>(
        &mut self,
        clt_w: &mut W,
        ups_r: &mut R,
        rsp_header: &HttpForwardRemoteResponse,
        icap_adapter: HttpResponseAdapter<ServerIdleChecker>,
        adaptation_state: &mut RespmodAdaptationRunState,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Send + Unpin,
        W: AsyncWrite + Send + Unpin,
    {
        let mut log_interval = self.ctx.get_log_interval();
        let mut adaptation_fut = icap_adapter
            .xfer(adaptation_state, self.req, rsp_header, ups_r, clt_w)
            .boxed();
        loop {
            tokio::select! {
                biased;

                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                        log_ctx.log_periodic();
                    }
                }
                r = &mut adaptation_fut => {
                    return match r {
                        Ok(RespmodAdaptationEndState::OriginalTransferred) => {
                            self.http_notes.rsp_status = rsp_header.code;
                            Ok(())
                        }
                        Ok(RespmodAdaptationEndState::AdaptedTransferred(adapted_rsp)) => {
                            self.http_notes.rsp_status = adapted_rsp.code;
                            Ok(())
                        }
                        Err(e) => Err(e.into()),
                    }
                }
            }
        }
    }

    async fn send_response_without_adaptation<R, W>(
        &mut self,
        clt_w: &mut W,
        ups_r: &mut R,
        rsp_header: &HttpForwardRemoteResponse,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        self.send_error_response = false;

        if let Some(body_type) = rsp_header.body_type(&self.req.method) {
            let mut buf = Vec::with_capacity(self.ctx.server_config.tcp_copy.buffer_size());
            rsp_header.serialize_to(&mut buf);
            self.http_notes.rsp_status = rsp_header.code;
            self.send_response_body(buf, clt_w, ups_r, body_type).await
        } else {
            self.send_response_header(clt_w, rsp_header).await?;
            self.http_notes.rsp_status = rsp_header.code;
            self.http_notes.mark_rsp_no_body();
            Ok(())
        }
    }

    async fn send_response_body<R, W>(
        &mut self,
        header: Vec<u8>,
        clt_w: &mut W,
        ups_r: &mut R,
        body_type: HttpBodyType,
    ) -> ServerTaskResult<()>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let header_len = header.len() as u64;
        let mut body_reader = HttpBodyReader::new(
            ups_r,
            body_type,
            self.ctx.server_config.h1.body_line_max_len,
        );

        let mut ups_to_clt = StreamCopy::with_data(
            &mut body_reader,
            clt_w,
            &self.ctx.server_config.tcp_copy,
            header,
        );

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut log_interval = self.ctx.get_log_interval();
        let mut idle_count = 0;

        macro_rules! record_progress {
            () => {{
                let read = ups_to_clt.reader().body_size();
                // a chunked body is copied as on-wire bytes, so the size sent
                // to the client is a lower bound: the payload read, less everything
                // still buffered, as all of it could be payload
                let written = if ups_to_clt.reader().is_chunked() {
                    read.saturating_sub(ups_to_clt.cached_data_size())
                } else {
                    ups_to_clt.copied_size().saturating_sub(header_len)
                };
                self.http_notes.record_h1_rsp_body_progress(read, written)
            }};
        }
        loop {
            tokio::select! {
                biased;

                r = &mut ups_to_clt => {
                    return match r {
                        Ok(_) => {
                            self.http_notes.mark_rsp_recv_all();
                            let n = ups_to_clt.reader().body_size();
                            self.http_notes.ups_rsp_body_size = Some(n);
                            self.http_notes.clt_rsp_body_size = Some(n);
                            // clt_w is already flushed
                            Ok(())
                        }
                        Err(e) => {
                            if matches!(&e, StreamCopyError::ReadFailed(_))
                                && ups_to_clt.copied_size() < header_len
                            {
                                let _ = ups_to_clt.write_flush().await; // flush rsp header to client
                            }
                            record_progress!();
                            Err(match e {
                                StreamCopyError::ReadFailed(e) => {
                                    ServerTaskError::UpstreamReadFailed(e)
                                }
                                StreamCopyError::WriteFailed(e) => {
                                    ServerTaskError::ClientTcpWriteFailed(e)
                                }
                            })
                        }
                    };
                }
                _ = log_interval.tick() => {
                    if let Some(log_ctx) = self.get_log_context() {
                       log_ctx.log_periodic();
                    }
                }
                n = idle_interval.tick() => {
                    if ups_to_clt.is_idle() {
                        idle_count += n;

                        if idle_count >= self.max_idle_count {
                            record_progress!();
                            return if ups_to_clt.no_cached_data() {
                                Err(ServerTaskError::UpstreamAppTimeout("idle while reading response body"))
                            } else {
                                Err(ServerTaskError::ClientAppTimeout("idle while sending response body"))
                            };
                        }
                    } else {
                        idle_count = 0;

                        ups_to_clt.reset_active();
                    }

                    if self.ctx.server_quit_policy.force_quit() {
                        if ups_to_clt.copied_size() < header_len {
                            let _ = ups_to_clt.write_flush().await; // flush rsp header to client
                        }
                        record_progress!();
                        return Err(ServerTaskError::CanceledAsServerQuit)
                    }
                }
            }
        }
    }

    fn update_response_header(&mut self, rsp: &mut HttpForwardRemoteResponse) {
        if self.should_close {
            rsp.set_no_keep_alive();
        }

        if rsp.www_negotiate_auth() {
            self.origin_session_auth = true;
        }

        if let Some(_server_id) = &self.ctx.server_config.server_id {
            // TODO custom header
        }
    }

    async fn send_response_header<W>(
        &mut self,
        clt_w: &mut W,
        rsp: &HttpForwardRemoteResponse,
    ) -> ServerTaskResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let buf = rsp.serialize();
        clt_w
            .write_all_flush(buf.as_ref())
            .await
            .map_err(ServerTaskError::ClientTcpWriteFailed)
    }
}
