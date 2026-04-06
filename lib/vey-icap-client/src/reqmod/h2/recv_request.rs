/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 * Copyright 2026 VEY-OSS developers.
 */

use std::time::Duration;

use bytes::Bytes;
use h2::RecvStream;
use h2::client::SendRequest;
use h2::server::SendResponse;
use http::{Request, Response, StatusCode};

use vey_h2::{
    H2BodyTransfer, H2PreviewData, H2ResponseHeaderReceiver, H2StreamBodyTransferError,
    H2StreamFromChunkedTransfer, H2StreamFromChunkedTransferError, RequestExt,
};
use vey_http::server::HttpAdaptedRequest;
use vey_io_ext::IdleCheck;

use super::{
    H2ReqmodAdaptationError, H2RequestAdapter, ReqmodAdaptationEndState, ReqmodAdaptationMidState,
    ReqmodAdaptationRunState,
};
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H2RequestAdapter<I> {
    pub(super) async fn handle_original_http_request_without_body(
        self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_request: Request<()>,
        mut ups_send_req: SendRequest<Bytes>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<ReqmodAdaptationEndState, H2ReqmodAdaptationError> {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let (rsp_fut, _) = ups_send_req
            .send_request(http_request, true)
            .map_err(H2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();
        state.mark_ups_send_no_body();

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);
        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_send_rsp,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();

        Ok(ReqmodAdaptationEndState::OriginalTransferred(ups_rsp))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn handle_original_http_request_with_body(
        mut self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_request: Request<()>,
        preview_data: H2PreviewData,
        clt_body: RecvStream,
        mut ups_send_req: SendRequest<Bytes>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<ReqmodAdaptationEndState, H2ReqmodAdaptationError> {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let (rsp_fut, mut ups_send_stream) = ups_send_req
            .send_request(http_request, false)
            .map_err(H2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();

        // no reserve of capacity, let the driver buffer it
        preview_data
            .h2_unbounded_send_all(&mut ups_send_stream)
            .map_err(H2ReqmodAdaptationError::HttpUpstreamSendDataFailed)?;

        let mut body_transfer =
            H2BodyTransfer::new(clt_body, ups_send_stream, self.copy_config.yield_size());

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);

        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        fn convert_transfer_error(e: H2StreamBodyTransferError) -> H2ReqmodAdaptationError {
            match e {
                H2StreamBodyTransferError::RecvDataFailed(e)
                | H2StreamBodyTransferError::RecvTrailersFailed(e)
                | H2StreamBodyTransferError::ReleaseRecvCapacityFailed(e) => {
                    H2ReqmodAdaptationError::HttpClientRecvDataFailed(e)
                }
                H2StreamBodyTransferError::SendDataFailed(e)
                | H2StreamBodyTransferError::SendTrailersFailed(e)
                | H2StreamBodyTransferError::WaitSendCapacityFailed(e)
                | H2StreamBodyTransferError::GracefulCloseError(e) => {
                    H2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e)
                }
                H2StreamBodyTransferError::SenderNotInSendState => {
                    H2ReqmodAdaptationError::HttpUpstreamNotInSendState
                }
            }
        }

        loop {
            tokio::select! {
                biased;

                r = ups_recv_rsp.recv_header() => {
                    match r {
                        Ok(ups_rsp) => {
                            if let Some(final_rsp) = check_out_final_response(ups_rsp, clt_send_rsp, &mut self.allow_continue)? {
                                state.mark_ups_recv_header();
                                return if let Some(body) = ups_recv_rsp.take_body() {
                                    let (headers, _) = final_rsp.into_parts();
                                    let ups_rsp = Response::from_parts(headers, body);
                                    Ok(ReqmodAdaptationEndState::OriginalTransferred(ups_rsp))
                                } else {
                                    Err(H2ReqmodAdaptationError::UnsupportedInformationalResponse(
                                        final_rsp.status(),
                                    ))
                                };
                            }
                        }
                        Err(e) => return Err(H2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed(e)),
                    }
                }
                r = &mut body_transfer => {
                    match r {
                        Ok(_) => {
                            state.mark_ups_send_all();
                            break;
                        }
                        Err(e) => return Err(convert_transfer_error(e)),
                    }
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;

                        let quit = self.idle_checker.check_quit(idle_count);
                        if quit {
                            return if body_transfer.no_cached_data() {
                                Err(H2ReqmodAdaptationError::HttpClientReadIdle)
                            } else {
                                Err(H2ReqmodAdaptationError::HttpUpstreamWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;

                        body_transfer.reset_active();
                    }

                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H2ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }

        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_send_rsp,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();

        Ok(ReqmodAdaptationEndState::OriginalTransferred(ups_rsp))
    }

    pub(super) async fn recv_icap_http_request_without_body(
        mut self,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: Request<()>,
    ) -> Result<ReqmodAdaptationMidState, H2ReqmodAdaptationError> {
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

        let final_req = orig_http_request.adapt_to(&http_req);
        Ok(ReqmodAdaptationMidState::AdaptedRequest(
            http_req, final_req,
        ))
    }

    pub(super) async fn handle_icap_http_request_without_body(
        mut self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: Request<()>,
        mut ups_send_req: SendRequest<Bytes>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<ReqmodAdaptationEndState, H2ReqmodAdaptationError> {
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

        let final_req = orig_http_request.adapt_to(&http_req);

        let (rsp_fut, _) = ups_send_req
            .send_request(final_req, true)
            .map_err(H2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();
        state.mark_ups_send_no_body();

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);
        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_send_rsp,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();

        Ok(ReqmodAdaptationEndState::AdaptedTransferred(
            http_req, ups_rsp,
        ))
    }

    pub(super) async fn handle_icap_http_request_with_body_after_transfer(
        mut self,
        state: &mut ReqmodAdaptationRunState,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: Request<()>,
        mut ups_send_req: SendRequest<Bytes>,
        clt_send_rsp: &mut SendResponse<Bytes>,
    ) -> Result<ReqmodAdaptationEndState, H2ReqmodAdaptationError> {
        let http_req = HttpAdaptedRequest::parse(
            &mut self.icap_connection.reader,
            http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;

        let final_req = orig_http_request.adapt_to(&http_req);
        let (rsp_fut, mut ups_send_stream) = ups_send_req
            .send_request(final_req, false)
            .map_err(H2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();

        let mut body_transfer = H2StreamFromChunkedTransfer::new(
            &mut self.icap_connection.reader,
            &mut ups_send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);

        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = ups_recv_rsp.recv_header() => {
                    match r {
                        Ok(ups_rsp) => {
                            if let Some(final_rsp) = check_out_final_response(ups_rsp, clt_send_rsp, &mut self.allow_continue)? {
                                state.mark_ups_recv_header();
                                return if let Some(body) = ups_recv_rsp.take_body() {
                                    let (headers, _) = final_rsp.into_parts();
                                    if body_transfer.finished() {
                                        self.icap_connection.mark_reader_finished();
                                        if icap_rsp.keep_alive {
                                            self.icap_client.save_connection(self.icap_connection);
                                        }
                                    }
                                    let ups_rsp = Response::from_parts(headers, body);
                                    Ok(ReqmodAdaptationEndState::AdaptedTransferred(http_req, ups_rsp))
                                } else {
                                    Err(H2ReqmodAdaptationError::UnsupportedInformationalResponse(
                                        final_rsp.status(),
                                    ))
                                };
                            }
                        }
                        Err(e) => return Err(H2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed(e)),
                    }
                }
                r = &mut body_transfer => {
                    match r {
                        Ok(_) => {
                            state.mark_ups_send_all();
                            self.icap_connection.mark_reader_finished();
                            if icap_rsp.keep_alive {
                                self.icap_client.save_connection(self.icap_connection);
                            }
                            break;
                        }
                        Err(H2StreamFromChunkedTransferError::ReadError(e)) => return Err(H2ReqmodAdaptationError::IcapServerReadFailed(e)),
                        Err(H2StreamFromChunkedTransferError::SendDataFailed(e)) => return Err(H2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e)),
                        Err(H2StreamFromChunkedTransferError::SendTrailerFailed(e)) => return Err(H2ReqmodAdaptationError::HttpUpstreamSendTrailedFailed(e)),
                        Err(H2StreamFromChunkedTransferError::SenderNotInSendState) => return Err(H2ReqmodAdaptationError::HttpUpstreamNotInSendState),
                    }
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;

                        let quit = self.idle_checker.check_quit(idle_count);
                        if quit {
                            return if body_transfer.no_cached_data() {
                                Err(H2ReqmodAdaptationError::HttpClientReadIdle)
                            } else {
                                Err(H2ReqmodAdaptationError::HttpUpstreamWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;

                        body_transfer.reset_active();
                    }

                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H2ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }

        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_send_rsp,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();

        Ok(ReqmodAdaptationEndState::AdaptedTransferred(
            http_req, ups_rsp,
        ))
    }
}

pub(super) async fn recv_ups_response_head_after_transfer(
    ups_recv_rsp: &mut H2ResponseHeaderReceiver,
    clt_send_rsp: &mut SendResponse<Bytes>,
    allow_continue: bool,
    timeout: Duration,
) -> Result<Response<RecvStream>, H2ReqmodAdaptationError> {
    tokio::time::timeout(
        timeout,
        recv_final_response_after_transfer(ups_recv_rsp, clt_send_rsp, allow_continue),
    )
    .await
    .map_err(|_| H2ReqmodAdaptationError::HttpUpstreamRecvResponseTimeout)?
}

async fn recv_final_response_after_transfer(
    ups_recv_rsp: &mut H2ResponseHeaderReceiver,
    clt_send_rsp: &mut SendResponse<Bytes>,
    mut allow_continue: bool,
) -> Result<Response<RecvStream>, H2ReqmodAdaptationError> {
    loop {
        let rsp = ups_recv_rsp
            .recv_header()
            .await
            .map_err(H2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed)?;
        if let Some(final_rsp) = check_out_final_response(rsp, clt_send_rsp, &mut allow_continue)? {
            return if let Some(body) = ups_recv_rsp.take_body() {
                let (headers, _) = final_rsp.into_parts();
                Ok(Response::from_parts(headers, body))
            } else {
                Err(H2ReqmodAdaptationError::UnsupportedInformationalResponse(
                    final_rsp.status(),
                ))
            };
        }
    }
}

pub(super) fn check_out_final_response(
    rsp: Response<()>,
    clt_send_rsp: &mut SendResponse<Bytes>,
    allow_continue: &mut bool,
) -> Result<Option<Response<()>>, H2ReqmodAdaptationError> {
    match rsp.status() {
        StatusCode::CONTINUE => {
            if *allow_continue {
                clt_send_rsp
                    .send_informational(rsp)
                    .map_err(H2ReqmodAdaptationError::HttpClientSendResponseFailed)?;
                *allow_continue = false;
            } else {
                return Err(H2ReqmodAdaptationError::InvalidUpstreamContinueResponse);
            }
        }
        StatusCode::EARLY_HINTS => {
            clt_send_rsp
                .send_informational(rsp)
                .map_err(H2ReqmodAdaptationError::HttpClientSendResponseFailed)?;
        }
        _ => return Ok(Some(rsp)),
    }
    Ok(None)
}
