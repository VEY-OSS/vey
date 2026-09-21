/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_h2::{H2PreviewData, H2StreamToChunkedTransfer};
use vey_http::server::HttpAdaptedRequest;
use vey_http::{HttpBodyDecodeReader, HttpBodyReader};
use vey_io_ext::{IdleCheck, StreamCopy, StreamCopyError};

use super::{
    H2ToH1ReqmodAdaptationError, H2ToH1RequestAdapter, HttpRequestForAdaptation,
    HttpRequestUpstreamWriter, ReqmodAdaptationEndState, ReqmodAdaptationMidState,
    ReqmodAdaptationRunState,
};
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H2ToH1RequestAdapter<I> {
    pub(super) async fn handle_original_http_request_without_body<H, UW>(
        self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_request: &H,
        ups_writer: &mut UW,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        ups_writer
            .send_request_header(http_request)
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        ups_writer
            .flush()
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        state.mark_ups_send_header();
        state.mark_ups_send_no_body();
        Ok(ReqmodAdaptationEndState::OriginalTransferred)
    }

    pub(super) async fn recv_icap_http_request_without_body<H>(
        mut self,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: &H,
    ) -> Result<ReqmodAdaptationMidState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
    {
        let http_req = HttpAdaptedRequest::parse(
            &mut self.icap_connection.reader,
            http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let final_req = orig_http_request.adapt_without_body(http_req);

        Ok(ReqmodAdaptationMidState::AdaptedRequest(final_req))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_original_http_request_with_body<H, UW>(
        self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_request: &H,
        mut preview_data: H2PreviewData,
        clt_body: &mut h2::RecvStream,
        ups_writer: &mut UW,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        ups_writer
            .send_request_header(http_request)
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        state.mark_ups_send_header();

        let preview_size = preview_data.preview_size() as u64;
        preview_data
            .write_as_chunk(ups_writer)
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;

        let mut body_transfer = if let Some(left) = preview_data.take_left() {
            H2StreamToChunkedTransfer::with_chunk(
                clt_body,
                ups_writer,
                self.copy_config.yield_size(),
                left,
            )
        } else {
            H2StreamToChunkedTransfer::new(clt_body, ups_writer, self.copy_config.yield_size())
        };
        body_transfer.add_copied(preview_size);

        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;
                r = &mut body_transfer => {
                    return match r {
                        Ok(_) => {
                            state.mark_ups_send_all();
                            let n = body_transfer.copied_size();
                            state.clt_req_body_size = Some(n);
                            state.ups_req_body_size = Some(n);
                            Ok(ReqmodAdaptationEndState::OriginalTransferred)
                        }
                        Err(e) => {
                            state.clt_req_body_size = Some(body_transfer.received_size());
                            state.ups_req_body_size = Some(body_transfer.copied_size());
                            Err(H2ToH1ReqmodAdaptationError::clt_to_upstream(e))
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            state.clt_req_body_size = Some(body_transfer.received_size());
                            state.ups_req_body_size = Some(body_transfer.copied_size());
                            return if body_transfer.no_cached_data() {
                                Err(H2ToH1ReqmodAdaptationError::HttpClientReadIdle)
                            } else {
                                Err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        state.clt_req_body_size = Some(body_transfer.received_size());
                        state.ups_req_body_size = Some(body_transfer.copied_size());
                        return Err(H2ToH1ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    pub(super) async fn handle_icap_http_request_without_body<H, UW>(
        mut self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: &H,
        ups_writer: &mut UW,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        let http_req = HttpAdaptedRequest::parse(
            &mut self.icap_connection.reader,
            http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let final_req = orig_http_request.adapt_without_body(http_req);
        ups_writer
            .send_request_header(&final_req)
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        ups_writer
            .flush()
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        state.mark_ups_send_header();
        state.mark_ups_send_no_body();
        Ok(ReqmodAdaptationEndState::AdaptedTransferred(final_req))
    }

    pub(super) async fn handle_icap_http_request_with_body_after_transfer<H, UW>(
        mut self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: &H,
        ups_writer: &mut UW,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        let http_req = HttpAdaptedRequest::parse(
            &mut self.icap_connection.reader,
            http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;
        let body_content_length = http_req.content_length;

        let final_req = orig_http_request.adapt_with_body(http_req);
        ups_writer
            .send_request_header(&final_req)
            .await
            .map_err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        state.mark_ups_send_header();

        match body_content_length {
            Some(0) => Err(H2ToH1ReqmodAdaptationError::InvalidHttpBodyFromIcapServer(
                anyhow!("Content-Length is 0 but the ICAP server response contains http-body"),
            )),
            Some(expected) => {
                let mut body_reader = HttpBodyDecodeReader::new_chunked(
                    &mut self.icap_connection.reader,
                    self.http_body_line_max_size,
                );
                let mut body_copy =
                    StreamCopy::new(&mut body_reader, ups_writer, &self.copy_config);
                if let Err(e) = Self::send_request_body(&self.idle_checker, &mut body_copy).await {
                    state.ups_req_body_size = Some(body_copy.copied_size());
                    return Err(e);
                }
                state.mark_ups_send_all();
                state.ups_req_body_size = Some(body_reader.body_size());
                let copied = body_reader.body_size();
                if body_reader
                    .trailer(self.http_trailer_max_size)
                    .await
                    .is_ok()
                {
                    self.icap_connection.mark_reader_finished();
                    if icap_rsp.keep_alive {
                        self.icap_client.save_connection(self.icap_connection);
                    }
                }
                if copied != expected {
                    return Err(H2ToH1ReqmodAdaptationError::InvalidHttpBodyFromIcapServer(
                        anyhow!("Content-Length is {expected} but decoded length is {copied}"),
                    ));
                }
                Ok(ReqmodAdaptationEndState::AdaptedTransferred(final_req))
            }
            None => {
                let mut body_reader = HttpBodyReader::new_chunked(
                    &mut self.icap_connection.reader,
                    self.http_body_line_max_size,
                );
                let mut body_copy =
                    StreamCopy::new(&mut body_reader, ups_writer, &self.copy_config);
                if let Err(e) = Self::send_request_body(&self.idle_checker, &mut body_copy).await {
                    state.ups_req_body_size = Some(
                        body_copy
                            .reader()
                            .body_size()
                            .saturating_sub(body_copy.cached_data_size()),
                    );
                    return Err(e);
                }
                state.mark_ups_send_all();
                state.ups_req_body_size = Some(body_reader.body_size());
                self.icap_connection.mark_reader_finished();
                if icap_rsp.keep_alive {
                    self.icap_client.save_connection(self.icap_connection);
                }
                Ok(ReqmodAdaptationEndState::AdaptedTransferred(final_req))
            }
        }
    }

    async fn send_request_body<R, W>(
        idle_checker: &I,
        mut body_copy: &mut StreamCopy<'_, R, W>,
    ) -> Result<(), H2ToH1ReqmodAdaptationError>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let mut idle_interval = idle_checker.interval_timer();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;
                r = &mut body_copy => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(StreamCopyError::ReadFailed(e)) => {
                            Err(H2ToH1ReqmodAdaptationError::IcapServerReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => {
                            Err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed(e))
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if body_copy.is_idle() {
                        idle_count += n;
                        if idle_checker.check_quit(idle_count) {
                            return if body_copy.no_cached_data() {
                                Err(H2ToH1ReqmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H2ToH1ReqmodAdaptationError::HttpUpstreamWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_copy.reset_active();
                    }
                    if let Some(reason) = idle_checker.check_force_quit() {
                        return Err(H2ToH1ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }
}
