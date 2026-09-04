/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use log::trace;
use tokio::io::AsyncRead;
use tokio::sync::mpsc;

use vey_io_ext::{GlobalLimitGroup, LimitedBufReadExt, LimitedBufReader, NilLimitedStats};
use vey_types::net::{HttpForwardedHeaderType, HttpForwardedHeaderValue};

use super::protocol::{HttpClientReader, HttpExposeRequest};
use super::{
    CommonTaskContext, HttpExposeCltWrapperStats, HttpExposePipelineStats,
    HttpExposePipelineTaskGuard,
};
use crate::module::http_forward::HttpProxyClientResponse;
use crate::serve::ServerStats;

pub(crate) struct HttpExposePipelineReaderTask<CDR> {
    ctx: Arc<CommonTaskContext>,
    task_queue: mpsc::Sender<
        Result<(HttpExposeRequest<CDR>, HttpExposePipelineTaskGuard), HttpProxyClientResponse>,
    >,
    stream_reader: Option<HttpClientReader<CDR>>,
    pipeline_stats: Arc<HttpExposePipelineStats>,
}

impl<CDR> HttpExposePipelineReaderTask<CDR>
where
    CDR: AsyncRead + Send + Unpin + 'static,
{
    pub(crate) fn new(
        ctx: &Arc<CommonTaskContext>,
        task_sender: mpsc::Sender<
            Result<(HttpExposeRequest<CDR>, HttpExposePipelineTaskGuard), HttpProxyClientResponse>,
        >,
        read_half: CDR,
        pipeline_stats: &Arc<HttpExposePipelineStats>,
    ) -> Self {
        let clt_r_stats = HttpExposeCltWrapperStats::new_for_reader(&ctx.server_stats);
        let limit_config = &ctx.server_config.tcp_sock_speed_limit;
        let clt_r = LimitedBufReader::new(
            read_half,
            limit_config.shift_millis,
            limit_config.max_north,
            clt_r_stats,
            Arc::new(NilLimitedStats::default()),
        );
        HttpExposePipelineReaderTask {
            ctx: Arc::clone(ctx),
            task_queue: task_sender,
            stream_reader: Some(clt_r),
            pipeline_stats: Arc::clone(pipeline_stats),
        }
    }

    pub(crate) async fn into_running(mut self) {
        // NOTE the receiver end should not be cloned, as the closed events is bounding to each
        let task_queue = self.task_queue.clone(); // to avoid ref self
        tokio::select! {
            biased;

            _ = task_queue.closed() => {
                trace!("write end has closed for previous request");
            }
            _ = self.run() => {}
        }
    }

    fn append_forwarded(&self, req: &mut HttpExposeRequest<CDR>) {
        match self.ctx.server_config.append_forwarded_for {
            HttpForwardedHeaderType::Disable => {}
            HttpForwardedHeaderType::Classic => {
                let v = HttpForwardedHeaderValue::new_classic(self.ctx.client_ip());
                v.append_to(&mut req.inner.end_to_end_headers);
            }
            HttpForwardedHeaderType::Standard => {
                let v = HttpForwardedHeaderValue::new_standard(
                    self.ctx.client_addr(),
                    self.ctx.server_addr(),
                );
                v.append_to(&mut req.inner.end_to_end_headers);
            }
        }
    }

    async fn run(&mut self) {
        let (stream_sender, mut stream_receiver) = mpsc::channel(1);
        loop {
            if let Some(mut reader) = self.stream_reader.take() {
                let quit_after_timeout = self.pipeline_stats.get_alive_task() <= 0;

                match tokio::time::timeout(
                    self.ctx.server_config.pipeline_read_idle_timeout,
                    reader.fill_wait_data(),
                )
                .await
                {
                    Ok(Ok(true)) => {}
                    Ok(Ok(false)) => {
                        trace!("client {} closed", self.ctx.client_addr());
                        break;
                    }
                    Ok(Err(e)) => {
                        trace!("client {} closed with error {e:?}", self.ctx.client_addr());
                        break;
                    }
                    Err(_) => {
                        // timeout
                        self.stream_reader = Some(reader);
                        if quit_after_timeout {
                            // TODO may be attack
                            break;
                        }
                        continue;
                    }
                }

                let mut version: http::Version = http::Version::HTTP_11; // default to 1.1
                match tokio::time::timeout(
                    self.ctx.server_config.timeout.recv_req_header,
                    HttpExposeRequest::parse(
                        &mut reader,
                        stream_sender.clone(),
                        self.ctx.server_config.req_hdr_max_size,
                        self.ctx.server_config.server_id.as_ref(),
                        &mut version,
                    ),
                )
                .await
                {
                    Ok(Ok((mut req, send_reader))) => {
                        self.append_forwarded(&mut req);

                        if send_reader {
                            req.body_reader = Some(reader);
                        } else {
                            self.stream_reader = Some(reader);
                        }

                        let server_is_online = self.ctx.server_stats.is_online();
                        if !server_is_online {
                            // According to https://datatracker.ietf.org/doc/html/rfc7230#section-6.3.2
                            // A client that pipelines requests SHOULD retry unanswered requests if
                            // the connection closes before it receives all of the corresponding
                            // responses.
                            req.inner.disable_keep_alive();
                        }

                        if self
                            .task_queue
                            .send(Ok((req, self.pipeline_stats.add_task())))
                            .await
                            .is_err()
                        {
                            trace!(
                                "write end has closed for previous request while sending new request"
                            );
                            break;
                        }

                        if !server_is_online {
                            break;
                        }
                    }
                    Ok(Err(e)) => {
                        self.stream_reader = Some(reader);
                        if let Some(response) =
                            HttpProxyClientResponse::from_request_error(&e, version)
                            && self.task_queue.send(Err(response)).await.is_err()
                        {
                            trace!(
                                "write end has closed for previous request while sending error response"
                            );
                        }
                        trace!("Error handling client {}: {e:?}", self.ctx.client_addr());
                        // TODO handle error, negotiation failed, may be attack
                        break;
                    }
                    Err(_) => {
                        trace!("timeout to read in a complete request header");
                        // TODO handle timeout, may be attack
                        break;
                    }
                }
            } else {
                match stream_receiver.recv().await.flatten() {
                    Some(mut reader) => {
                        // we can now read the next request
                        reader.reset_buffer_stats(Arc::new(NilLimitedStats::default()));
                        let limit_config = &self.ctx.server_config.tcp_sock_speed_limit;
                        reader.reset_local_limit(limit_config.shift_millis, limit_config.max_north);
                        reader.retain_global_limiter_by_group(GlobalLimitGroup::Server);
                        self.stream_reader = Some(reader);
                    }
                    None => {
                        // write end closed normally, task done
                        break;
                    }
                }
            }
        }
    }
}
