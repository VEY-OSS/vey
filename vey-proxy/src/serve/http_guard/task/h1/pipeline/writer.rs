/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;

use vey_io_ext::{ArcLimitedWriterStats, LimitedWriter};
use vey_types::net::HttpUpgradeToken;
use vey_types::route::HostMatch;

use super::protocol::{HttpClientWriter, HttpGuardRequest};
use super::{
    H1TaskContext, HttpGuardCltWrapperStats, HttpGuardForwardTask, HttpGuardPipelineTaskGuard,
    HttpGuardWebsocketTask,
};
use crate::config::server::ServerConfig;
use crate::module::http_forward::{BoxHttpForwardContext, HttpProxyClientResponse};
use crate::serve::http_guard::HttpHost;
use crate::serve::{ServerStats, ServerTaskNotes};
use crate::site::{Site, SiteContext, SiteHttpConnGuard};

pub(crate) struct HttpGuardPipelineWriterTask<CDR, CDW> {
    ctx: Arc<H1TaskContext>,
    task_queue: mpsc::Receiver<
        Result<(HttpGuardRequest<CDR>, HttpGuardPipelineTaskGuard), HttpProxyClientResponse>,
    >,
    stream_writer: Option<HttpClientWriter<CDW>>,
    forward_context: BoxHttpForwardContext,
    wrapper_stats: ArcLimitedWriterStats,
    site_conn: Option<SiteHttpConnGuard>,
}

enum LoopAction {
    Continue,
    Break,
}

impl<CDR, CDW> HttpGuardPipelineWriterTask<CDR, CDW>
where
    CDR: AsyncRead + Send + Sync + Unpin + 'static,
    CDW: AsyncWrite + Send + Sync + Unpin + 'static,
{
    pub(crate) fn new(
        ctx: &Arc<H1TaskContext>,
        task_receiver: mpsc::Receiver<
            Result<(HttpGuardRequest<CDR>, HttpGuardPipelineTaskGuard), HttpProxyClientResponse>,
        >,
        write_half: CDW,
    ) -> Self {
        let forward_context = ctx
            .escaper
            .new_http_forward_context(Arc::clone(&ctx.escaper));
        let clt_w_stats = HttpGuardCltWrapperStats::new_for_writer(&ctx.server_stats);
        let limit_config = &ctx.server_config.tcp_sock_speed_limit;
        let clt_w = LimitedWriter::local_limited(
            write_half,
            limit_config.shift_millis,
            limit_config.max_south,
            Arc::clone(&clt_w_stats),
        );
        HttpGuardPipelineWriterTask {
            ctx: Arc::clone(ctx),
            task_queue: task_receiver,
            stream_writer: Some(clt_w),
            forward_context,
            wrapper_stats: clt_w_stats,
            site_conn: None,
        }
    }

    fn note_site_conn(&mut self, site: &Site) {
        if self.site_conn.is_some() {
            return;
        }
        self.site_conn = Some(site.hold_http_conn(
            self.ctx.server_config.name(),
            self.ctx.server_stats.share_extra_tags(),
        ));
    }

    pub(crate) async fn into_running(mut self, hosts: Arc<HostMatch<Arc<HttpHost>>>) {
        loop {
            let res = match self.task_queue.recv().await {
                Some(Ok((req, pipeline_task))) => {
                    let action = self.check_run(req, &hosts).await;
                    drop(pipeline_task);
                    action
                }
                Some(Err(mut rsp)) => {
                    if !self.ctx.server_config.no_early_error_reply
                        && let Some(stream_w) = &mut self.stream_writer
                    {
                        self.ctx.apply_proxy_status_ident(&mut rsp);
                        let _ = rsp.reply_err_to_request(stream_w).await;
                    }

                    self.notify_reader_to_close();
                    LoopAction::Break
                }
                None => LoopAction::Break,
            };
            match res {
                LoopAction::Continue => {}
                LoopAction::Break => {
                    break;
                }
            }
        }
    }

    async fn check_run(
        &mut self,
        req: HttpGuardRequest<CDR>,
        hosts: &HostMatch<Arc<HttpHost>>,
    ) -> LoopAction {
        match hosts.get_matched(req.upstream.host()) {
            Some(host) => match &self.ctx.site_ctx {
                Some(pinned) => {
                    if !host.same_site(pinned.site()) {
                        if !self.ctx.server_config.no_early_error_reply
                            && let Some(stream_w) = &mut self.stream_writer
                        {
                            let mut rsp =
                                HttpProxyClientResponse::misdirected_request(req.inner.version);
                            self.ctx.apply_proxy_status_ident(&mut rsp);
                            let _ = rsp.reply_err_to_request(stream_w).await;
                        }

                        self.notify_reader_to_close();
                        LoopAction::Break
                    } else {
                        let site_ctx = pinned.clone();
                        self.note_site_conn(host.site());
                        self.run(req, site_ctx).await
                    }
                }
                None => {
                    let site_ctx = SiteContext::new(
                        Arc::clone(host.site()),
                        Arc::clone(host.egress()),
                        self.ctx.server_config.name(),
                        self.ctx.server_stats.share_extra_tags(),
                    );
                    self.note_site_conn(host.site());
                    self.run(req, site_ctx).await
                }
            },
            None => {
                if !self.ctx.server_config.no_early_error_reply
                    && let Some(stream_w) = &mut self.stream_writer
                {
                    let mut rsp = HttpProxyClientResponse::bad_request(req.inner.version);
                    self.ctx.apply_proxy_status_ident(&mut rsp);
                    let _ = rsp.reply_err_to_request(stream_w).await;
                }

                self.notify_reader_to_close();
                LoopAction::Break
            }
        }
    }

    async fn run(&mut self, mut req: HttpGuardRequest<CDR>, site_ctx: SiteContext) -> LoopAction {
        let Some(mut stream_w) = self.stream_writer.take() else {
            unreachable!()
        };

        let task_notes =
            ServerTaskNotes::new(self.ctx.cc_info.clone(), None, req.time_accepted.elapsed())
                .with_site_ctx(site_ctx.clone());

        match req.inner.upgrade_token() {
            Some(HttpUpgradeToken::Websocket) => {
                let Some(mut stream_r) = req.body_reader.take() else {
                    unreachable!()
                };
                let mut ws_task =
                    HttpGuardWebsocketTask::new(&self.ctx, &req, site_ctx, task_notes);
                let connected = ws_task
                    .connect_to_origin(&req.inner, &mut stream_r, &mut stream_w)
                    .await;
                let _ = req.stream_sender.try_send(None);
                if let Some((ups_c, rsp)) = connected {
                    ws_task.into_running(stream_r, stream_w, ups_c, rsp).await;
                }
                LoopAction::Break
            }
            Some(_) => {
                if !self.ctx.server_config.no_early_error_reply {
                    let mut rsp = HttpProxyClientResponse::unimplemented(req.inner.version);
                    self.ctx.apply_proxy_status_ident(&mut rsp);
                    let _ = rsp.reply_err_to_request(&mut stream_w).await;
                }
                let _ = req.stream_sender.try_send(None);
                LoopAction::Break
            }
            None => {
                let site = site_ctx.site();
                let _ = self
                    .forward_context
                    .check_in_final_escaper(
                        &task_notes,
                        site.upstream(),
                        site.tls_client().is_some(),
                    )
                    .await;
                match self
                    .run_forward(&mut stream_w, req, site_ctx, task_notes)
                    .await
                {
                    LoopAction::Continue => {
                        self.reset_client_writer(stream_w);
                        LoopAction::Continue
                    }
                    LoopAction::Break => LoopAction::Break,
                }
            }
        }
    }

    fn reset_client_writer(&mut self, mut stream_w: HttpClientWriter<CDW>) {
        stream_w.reset_stats(Arc::clone(&self.wrapper_stats));
        let limit_config = &self.ctx.server_config.tcp_sock_speed_limit;
        stream_w.reset_local_limit(limit_config.shift_millis, limit_config.max_south);
        self.stream_writer = Some(stream_w);
    }

    async fn run_forward(
        &mut self,
        clt_w: &mut HttpClientWriter<CDW>,
        mut req: HttpGuardRequest<CDR>,
        site_ctx: SiteContext,
        task_notes: ServerTaskNotes,
    ) -> LoopAction {
        match req.body_reader.take() {
            Some(stream_r) => {
                let mut forward_task =
                    HttpGuardForwardTask::new(&self.ctx, &req, site_ctx, task_notes);
                let mut clt_r = Some(stream_r);
                forward_task
                    .run(&mut clt_r, clt_w, &mut self.forward_context)
                    .await;
                if forward_task.should_close() {
                    let _ = req.stream_sender.try_send(None);
                    LoopAction::Break
                } else if req.stream_sender.try_send(clt_r).is_err() {
                    LoopAction::Break
                } else {
                    LoopAction::Continue
                }
            }
            None => {
                let mut forward_task =
                    HttpGuardForwardTask::new(&self.ctx, &req, site_ctx, task_notes);
                let mut clt_r = None;
                forward_task
                    .run::<CDR, CDW>(&mut clt_r, clt_w, &mut self.forward_context)
                    .await;
                if forward_task.should_close() {
                    self.notify_reader_to_close();
                    LoopAction::Break
                } else {
                    LoopAction::Continue
                }
            }
        }
    }

    fn notify_reader_to_close(&mut self) {
        self.task_queue.close();
    }
}
