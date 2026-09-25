/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use bytes::Bytes;
use http::Response;
use tokio::io::AsyncBufRead;

use vey_h2::{H2BodyEncodeTransfer, H2StreamFromChunkedTransfer, ResponseExt};
use vey_http::client::HttpAdaptedResponse;
use vey_http::{HttpBodyDecodeReader, HttpBodyType};
use vey_io_ext::IdleCheck;

use super::{
    H1ToH2RespmodAdaptationError, H1ToH2ResponseAdapter, H2SendResponseToClient,
    RespmodAdaptationEndState, RespmodAdaptationRunState,
};
use crate::reason::IcapErrorReason;
use crate::respmod::response::RespmodResponse;

impl<I: IdleCheck> H1ToH2ResponseAdapter<I> {
    pub(super) async fn handle_icap_ok_without_payload(
        self,
        icap_rsp: RespmodResponse,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError> {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
            IcapErrorReason::NoBodyFound,
            icap_rsp.code,
            icap_rsp.reason,
        ))
    }

    pub(super) async fn handle_original_http_response_without_body<CW>(
        self,
        state: &mut RespmodAdaptationRunState,
        icap_rsp: RespmodResponse,
        http_response: Response<()>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        CW: H2SendResponseToClient,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        state.mark_clt_send_start();
        clt_send_response
            .send_response(http_response, true)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();
        state.mark_clt_send_no_body();
        Ok(RespmodAdaptationEndState::OriginalTransferred)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_original_http_response_with_body<UR, CW>(
        self,
        state: &mut RespmodAdaptationRunState,
        icap_rsp: RespmodResponse,
        http_response: Response<()>,
        ups_body_type: HttpBodyType,
        ups_body_io: &mut UR,
        preview_buf: Vec<u8>,
        left_chunk_size: u64,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        state.mark_clt_send_start();
        let mut clt_send_stream = clt_send_response
            .send_response(http_response, false)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();

        let preview_len = preview_buf.len() as u64;
        if !preview_buf.is_empty() {
            // no reserve of capacity, let the driver buffer it
            clt_send_stream
                .send_data(Bytes::from(preview_buf), false)
                .map_err(H1ToH2RespmodAdaptationError::HttpClientSendDataFailed)?;
        }

        match ups_body_type {
            HttpBodyType::Chunked => {
                let mut body_transfer = H2StreamFromChunkedTransfer::resume(
                    ups_body_io,
                    &mut clt_send_stream,
                    &self.copy_config,
                    self.http_body_line_max_size,
                    self.http_trailer_max_size,
                    left_chunk_size,
                );

                let mut idle_interval = self.idle_checker.interval_timer();
                let mut idle_count = 0;

                loop {
                    tokio::select! {
                        biased;

                        r = &mut body_transfer => {
                            match r {
                                Ok(_) => {
                                    let n = preview_len + body_transfer.copied_size();
                                    state.ups_rsp_body_size = Some(n);
                                    state.clt_rsp_body_size = Some(n);
                                    break;
                                }
                                Err(e) => {
                                    state.ups_rsp_body_size =
                                        Some(preview_len + body_transfer.received_size());
                                    state.clt_rsp_body_size =
                                        Some(preview_len + body_transfer.copied_size());
                                    return Err(H1ToH2RespmodAdaptationError::ups_to_clt(e));
                                }
                            }
                        }
                        n = idle_interval.tick() => {
                            if body_transfer.is_idle() {
                                idle_count += n;

                                if self.idle_checker.check_quit(idle_count) {
                                    state.ups_rsp_body_size =
                                        Some(preview_len + body_transfer.received_size());
                                    state.clt_rsp_body_size =
                                        Some(preview_len + body_transfer.copied_size());
                                    return if body_transfer.no_cached_data() {
                                        Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                                    } else {
                                        Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                                    };
                                }
                            } else {
                                idle_count = 0;

                                body_transfer.reset_active();
                            }

                            if let Some(reason) = self.idle_checker.check_force_quit() {
                                state.ups_rsp_body_size =
                                    Some(preview_len + body_transfer.received_size());
                                state.clt_rsp_body_size =
                                    Some(preview_len + body_transfer.copied_size());
                                return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                            }
                        }
                    }
                }
            }
            HttpBodyType::ContentLength(_) | HttpBodyType::ReadUntilEnd => {
                let mut body_reader = HttpBodyDecodeReader::new(
                    ups_body_io,
                    ups_body_type,
                    self.http_body_line_max_size,
                );
                let mut body_transfer = H2BodyEncodeTransfer::new(
                    &mut body_reader,
                    &mut clt_send_stream,
                    &self.copy_config,
                );

                let mut idle_interval = self.idle_checker.interval_timer();
                let mut idle_count = 0;

                loop {
                    tokio::select! {
                        biased;

                        r = &mut body_transfer => {
                            match r {
                                Ok(_) => {
                                    let n = preview_len + body_transfer.copied_size();
                                    state.ups_rsp_body_size = Some(n);
                                    state.clt_rsp_body_size = Some(n);
                                    break;
                                }
                                Err(e) => {
                                    state.ups_rsp_body_size =
                                        Some(preview_len + body_transfer.received_size());
                                    state.clt_rsp_body_size =
                                        Some(preview_len + body_transfer.copied_size());
                                    return Err(e.into());
                                }
                            }
                        }
                        n = idle_interval.tick() => {
                            if body_transfer.is_idle() {
                                idle_count += n;

                                if self.idle_checker.check_quit(idle_count) {
                                    state.ups_rsp_body_size =
                                        Some(preview_len + body_transfer.received_size());
                                    state.clt_rsp_body_size =
                                        Some(preview_len + body_transfer.copied_size());
                                    return if body_transfer.no_cached_data() {
                                        Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                                    } else {
                                        Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                                    };
                                }
                            } else {
                                idle_count = 0;

                                body_transfer.reset_active();
                            }

                            if let Some(reason) = self.idle_checker.check_force_quit() {
                                state.ups_rsp_body_size =
                                    Some(preview_len + body_transfer.received_size());
                                state.clt_rsp_body_size =
                                    Some(preview_len + body_transfer.copied_size());
                                return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                            }
                        }
                    }
                }
                drop(body_transfer);
                clt_send_stream
                    .send_data(Bytes::new(), true)
                    .map_err(H1ToH2RespmodAdaptationError::HttpClientSendDataFailed)?;
            }
        }

        state.mark_ups_recv_all();
        state.mark_clt_send_all();
        Ok(RespmodAdaptationEndState::OriginalTransferred)
    }

    pub(super) async fn handle_icap_http_response_without_body<CW>(
        mut self,
        state: &mut RespmodAdaptationRunState,
        icap_rsp: RespmodResponse,
        http_header_size: usize,
        orig_http_response: Response<()>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        CW: H2SendResponseToClient,
    {
        let http_rsp =
            HttpAdaptedResponse::parse(&mut self.icap_connection.reader, http_header_size).await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let final_rsp = orig_http_response.adapt_to(&http_rsp);
        state.mark_clt_send_start();
        clt_send_response
            .send_response(final_rsp, true)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();
        state.mark_clt_send_no_body();
        Ok(RespmodAdaptationEndState::AdaptedTransferred(http_rsp))
    }

    pub(super) async fn handle_icap_http_response_with_body_after_transfer<CW>(
        mut self,
        state: &mut RespmodAdaptationRunState,
        icap_rsp: RespmodResponse,
        http_header_size: usize,
        orig_http_response: Response<()>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        CW: H2SendResponseToClient,
    {
        let http_rsp =
            HttpAdaptedResponse::parse(&mut self.icap_connection.reader, http_header_size).await?;

        let final_rsp = orig_http_response.adapt_to(&http_rsp);
        state.mark_clt_send_start();
        let mut clt_send_stream = clt_send_response
            .send_response(final_rsp, false)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();

        let mut body_transfer = H2StreamFromChunkedTransfer::new(
            &mut self.icap_connection.reader,
            &mut clt_send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                biased;
                r = &mut body_transfer => {
                    return match r {
                        Ok(_) => {
                            state.mark_clt_send_all();
                            state.clt_rsp_body_size = Some(body_transfer.copied_size());
                            self.icap_connection.mark_reader_finished();
                            if icap_rsp.keep_alive {
                                self.icap_client.save_connection(self.icap_connection);
                            }
                            Ok(RespmodAdaptationEndState::AdaptedTransferred(http_rsp))
                        }
                        Err(e) => {
                            state.clt_rsp_body_size = Some(body_transfer.copied_size());
                            Err(e.into())
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            state.clt_rsp_body_size = Some(body_transfer.copied_size());
                            return if body_transfer.no_cached_data() {
                                Err(H1ToH2RespmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        state.clt_rsp_body_size = Some(body_transfer.copied_size());
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }
}
