/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};

use vey_http::HttpBodyReader;
use vey_http::server::HttpProxyClientRequest;
use vey_io_ext::{StreamCopy, StreamCopyError};

use super::protocol::{HttpClientReader, HttpClientWriter, HttpExposeRequest};
use super::{CommonTaskContext, UntrustedCltReadWrapperStats};
use crate::config::server::ServerConfig;
use crate::module::http_forward::HttpProxyClientResponse;
use crate::serve::http_expose::HttpUntrustedTaskAliveGuard;
use crate::serve::{ServerStats, ServerTaskError, ServerTaskNotes, ServerTaskResult};
use crate::site::SiteContext;

pub(crate) struct HttpExposeUntrustedTask<'a> {
    ctx: Arc<CommonTaskContext>,
    site_ctx: SiteContext,
    req: &'a HttpProxyClientRequest,
    should_close: bool,
    task_notes: ServerTaskNotes,
    max_idle_count: usize,
    _alive_guard: Option<HttpUntrustedTaskAliveGuard>,
}

impl<'a> HttpExposeUntrustedTask<'a> {
    pub(crate) fn new(
        ctx: &Arc<CommonTaskContext>,
        req: &'a HttpExposeRequest<impl AsyncRead>,
        site_ctx: SiteContext,
    ) -> Self {
        let task_notes =
            ServerTaskNotes::new(ctx.cc_info.clone(), None, req.time_accepted.elapsed())
                .with_site_ctx(site_ctx.clone());
        let max_idle_count = task_notes.task_max_idle_count(ctx.server_config.task_idle_max_count);
        HttpExposeUntrustedTask {
            ctx: Arc::clone(ctx),
            site_ctx,
            req: &req.inner,
            should_close: !req.inner.keep_alive(),
            task_notes,
            max_idle_count,
            _alive_guard: None,
        }
    }

    fn pre_start(&mut self) {
        self._alive_guard = Some(self.ctx.server_stats.add_untrusted_task());
    }

    #[inline]
    pub(crate) fn should_close(&self) -> bool {
        self.should_close
    }

    pub(crate) async fn run<CDR, CDW>(
        &mut self,
        clt_r: &mut Option<HttpClientReader<CDR>>,
        clt_w: &mut HttpClientWriter<CDW>,
    ) where
        CDR: AsyncRead + Unpin,
        CDW: AsyncWrite + Unpin,
    {
        if self.task_notes.check_layered_rate_limit().is_err()
            || self.task_notes.acquire_site_request_semaphores().is_err()
        {
            self.should_close = true;
            self.reply_too_many_requests(clt_w).await;
            return;
        }

        let site_io = self.site_ctx.fetch_traffic_stats(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        );
        site_io
            .io
            .http_forward
            .add_in_bytes(self.req.origin_header_size() as u64);

        if self.req.body_type().is_none() {
            self.reply_auth_error(clt_w).await;
        } else if let Some(br) = clt_r {
            if self.req.has_auth_info() || self.ctx.server_config.untrusted_read_limit.is_none() {
                // untrusted read is not permitted, we should close the connection
                self.should_close = true;
            }

            self.reply_auth_error(clt_w).await;

            if !self.should_close
                && let Some(limit_config) = &self.ctx.server_config.untrusted_read_limit
            {
                let limit = self
                    .site_ctx
                    .tcp_sock_speed_limit()
                    .shrink_as_smaller(limit_config);
                br.reset_local_limit(limit.shift_millis, limit.max_north);
                let buffer_stats =
                    UntrustedCltReadWrapperStats::new_obj(&self.ctx.server_stats, site_io);
                br.reset_buffer_stats(buffer_stats);

                self.pre_start();
                if self.drain_body(br).await.is_err() {
                    self.should_close = true;
                }
            }
        } else {
            // should be impossible
            self.should_close = true;
        }
    }

    async fn reply_too_many_requests<CDW>(&mut self, clt_w: &mut HttpClientWriter<CDW>)
    where
        CDW: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::too_many_requests(self.req.version);
        self.ctx.apply_proxy_status_ident(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_err() {
            self.should_close = true;
        }
    }

    async fn reply_auth_error<CDW>(&mut self, clt_w: &mut HttpClientWriter<CDW>)
    where
        CDW: AsyncWrite + Unpin,
    {
        let mut rsp = HttpProxyClientResponse::need_login(
            self.req.version,
            self.should_close,
            self.ctx.server_config.auth_realm.as_str(),
        );
        self.ctx.apply_proxy_status_ident(&mut rsp);
        if rsp.reply_err_to_request(clt_w).await.is_err() {
            self.should_close = true;
        }
    }

    async fn drain_body<CDR>(&mut self, clt_r: &mut HttpClientReader<CDR>) -> ServerTaskResult<()>
    where
        CDR: AsyncRead + Unpin,
    {
        let mut body_reader = HttpBodyReader::new(
            clt_r,
            self.req.body_type().unwrap(),
            self.ctx.server_config.body_line_max_len,
        );
        let mut sink_w = tokio::io::sink();
        let mut clt_to_sink = StreamCopy::new(
            &mut body_reader,
            &mut sink_w,
            &self.ctx.server_config.tcp_copy,
        );

        let mut idle_interval = self.ctx.idle_wheel.register();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;

                r = &mut clt_to_sink => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(StreamCopyError::ReadFailed(e)) => Err(ServerTaskError::ClientTcpReadFailed(e)),
                        Err(StreamCopyError::WriteFailed(_)) => Err(ServerTaskError::InternalServerError("write to sinking failed")),
                    };
                }
                n = idle_interval.tick() => {
                    if clt_to_sink.is_idle() {
                        idle_count += n;

                        if idle_count >= self.max_idle_count {
                            return if clt_to_sink.no_cached_data() {
                                Err(ServerTaskError::ClientAppTimeout("idle while reading request body"))
                            } else {
                                Err(ServerTaskError::InternalServerError("idle while writing to sinking"))
                            };
                        }
                    } else {
                        idle_count = 0;

                        clt_to_sink.reset_active();
                    }

                    if self.ctx.server_quit_policy.force_quit() {
                        return Err(ServerTaskError::CanceledAsServerQuit)
                    }
                }
            }
        }
    }
}
