/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::io::AsyncBufRead;

use vey_h2::H2StreamFromChunkedTransfer;
use vey_http::H1BodyToChunkedTransfer;
use vey_http::client::HttpAdaptedResponse;
use vey_io_ext::{IdleCheck, LimitedBufReadExt, StreamCopyConfig, StreamCopyError};

use super::super::adapt_response_to_h2;
use super::client::map_from_chunked_err;
use super::{
    H1ToH2RespmodAdaptationError, H1ToH2RespmodEndState, H1ToH2RespmodRunState, orig_h2_response,
};
use crate::respmod::h1::HttpResponseForAdaptation;
use crate::respmod::h2::H2SendResponseToClient;
use crate::respmod::response::RespmodResponse;
use crate::{IcapClientReader, IcapClientWriter, IcapServiceClient};

pub(super) struct BidirectionalRecvIcapResponse<'a, I: IdleCheck> {
    pub(super) icap_client: &'a Arc<IcapServiceClient>,
    pub(super) icap_reader: &'a mut IcapClientReader,
    pub(super) idle_checker: &'a I,
}

impl<I: IdleCheck> BidirectionalRecvIcapResponse<'_, I> {
    pub(super) async fn transfer_and_recv<UR>(
        self,
        mut body_transfer: &mut H1BodyToChunkedTransfer<'_, UR, IcapClientWriter>,
    ) -> Result<RespmodResponse, H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
    {
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = &mut body_transfer => {
                    return match r {
                        Ok(_) => self.recv_icap_response().await,
                        Err(StreamCopyError::ReadFailed(e)) => {
                            Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => {
                            Err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed(e))
                        }
                    };
                }
                r = self.icap_reader.fill_wait_data() => {
                    return match r {
                        Ok(true) => self.recv_icap_response().await,
                        Ok(false) => Err(H1ToH2RespmodAdaptationError::IcapServerConnectionClosed),
                        Err(e) => Err(H1ToH2RespmodAdaptationError::IcapServerReadFailed(e)),
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if body_transfer.no_cached_data() {
                                Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                            } else {
                                Err(H1ToH2RespmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    async fn recv_icap_response(self) -> Result<RespmodResponse, H1ToH2RespmodAdaptationError> {
        let rsp = RespmodResponse::parse(
            self.icap_reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;
        Ok(rsp)
    }
}

pub(super) struct BidirectionalRecvHttpResponse<'a, I: IdleCheck> {
    pub(super) icap_reader: &'a mut IcapClientReader,
    pub(super) copy_config: StreamCopyConfig,
    pub(super) http_body_line_max_size: usize,
    pub(super) http_trailer_max_size: usize,
    pub(super) idle_checker: &'a I,
    pub(super) http_header_size: usize,
    pub(super) icap_read_finished: bool,
}

impl<I: IdleCheck> BidirectionalRecvHttpResponse<'_, I> {
    pub(super) async fn transfer<H, UR, CW>(
        &mut self,
        state: &mut H1ToH2RespmodRunState,
        mut ups_body_transfer: &mut H1BodyToChunkedTransfer<'_, UR, IcapClientWriter>,
        orig_http_response: &H,
        clt_send_response: &mut CW,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        let http_rsp = HttpAdaptedResponse::parse(self.icap_reader, self.http_header_size).await?;
        let final_rsp = adapt_response_to_h2(orig_h2_response(orig_http_response), &http_rsp);
        state.mark_clt_send_start();
        let mut clt_send_stream = clt_send_response
            .send_response(final_rsp, false)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();

        let mut adp_body_transfer = H2StreamFromChunkedTransfer::new(
            self.icap_reader,
            &mut clt_send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );

        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                r = &mut ups_body_transfer => {
                    return match r {
                        Ok(_) => {
                            match adp_body_transfer.await {
                                Ok(_) => {
                                    state.mark_clt_send_all();
                                    self.icap_read_finished = true;
                                    Ok(H1ToH2RespmodEndState::AdaptedTransferred(http_rsp))
                                }
                                Err(e) => Err(map_from_chunked_err(e)),
                            }
                        }
                        Err(StreamCopyError::ReadFailed(e)) => {
                            Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => {
                            Err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed(e))
                        }
                    };
                }
                r = &mut adp_body_transfer => {
                    return match r {
                        Ok(_) => {
                            state.mark_clt_send_all();
                            self.icap_read_finished = true;
                            Ok(H1ToH2RespmodEndState::AdaptedTransferred(http_rsp))
                        }
                        Err(e) => Err(map_from_chunked_err(e)),
                    };
                }
                n = idle_interval.tick() => {
                    if ups_body_transfer.is_idle() && adp_body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if ups_body_transfer.is_idle() {
                                if ups_body_transfer.no_cached_data() {
                                    Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                                } else {
                                    Err(H1ToH2RespmodAdaptationError::IcapServerWriteIdle)
                                }
                            } else if adp_body_transfer.no_cached_data() {
                                Err(H1ToH2RespmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        ups_body_transfer.reset_active();
                        adp_body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }
}
