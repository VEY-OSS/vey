/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use http::Response;
use tokio::io::AsyncBufRead;

use vey_h2::{H2StreamFromChunkedTransfer, ResponseExt};
use vey_http::H1BodyToChunkedTransfer;
use vey_http::client::HttpAdaptedResponse;
use vey_io_ext::{IdleCheck, LimitedBufReadExt, StreamCopyConfig, StreamCopyError};

use super::{
    H1ToH2RespmodAdaptationError, H2SendResponseToClient, RespmodAdaptationEndState,
    RespmodAdaptationRunState,
};
use crate::reason::IcapErrorReason;
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
        state: &mut RespmodAdaptationRunState,
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
                        Ok(_) => {
                            state.mark_ups_recv_all();
                            state.ups_rsp_body_size = Some(body_transfer.body_size());
                            self.recv_icap_response().await
                        }
                        Err(e) => {
                            state.ups_rsp_body_size = Some(body_transfer.body_size());
                            if body_transfer.reader_finished() {
                                state.mark_ups_recv_all();
                            }
                            match e {
                                StreamCopyError::ReadFailed(e) => {
                                    Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e))
                                }
                                StreamCopyError::WriteFailed(e) => {
                                    Err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed(e))
                                }
                            }
                        }
                    };
                }
                r = self.icap_reader.fill_wait_data() => {
                    return match r {
                        Ok(true) => self.recv_icap_response().await,
                        Ok(false) => {
                            state.ups_rsp_body_size = Some(body_transfer.body_size());
                            Err(H1ToH2RespmodAdaptationError::IcapServerConnectionClosed)
                        }
                        Err(e) => {
                            state.ups_rsp_body_size = Some(body_transfer.body_size());
                            Err(H1ToH2RespmodAdaptationError::IcapServerReadFailed(e))
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            state.ups_rsp_body_size = Some(body_transfer.body_size());
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
                        state.ups_rsp_body_size = Some(body_transfer.body_size());
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

        match rsp.code {
            204 | 206 => Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
                IcapErrorReason::InvalidResponseAfterContinue,
                rsp.code,
                rsp.reason,
            )),
            n if (200..300).contains(&n) => Ok(rsp),
            _ => Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
                IcapErrorReason::UnknownResponseAfterContinue,
                rsp.code,
                rsp.reason,
            )),
        }
    }
}

pub(super) struct BidirectionalRecvHttpResponse<'a, I: IdleCheck> {
    pub(super) http_body_line_max_size: usize,
    pub(super) http_trailer_max_size: usize,
    pub(super) copy_config: StreamCopyConfig,
    pub(super) idle_checker: &'a I,
    pub(super) http_header_size: usize,
    pub(super) icap_read_finished: bool,
}

impl<I: IdleCheck> BidirectionalRecvHttpResponse<'_, I> {
    pub(super) async fn transfer<UR, CW>(
        &mut self,
        state: &mut RespmodAdaptationRunState,
        ups_body_transfer: &mut H1BodyToChunkedTransfer<'_, UR, IcapClientWriter>,
        icap_reader: &mut IcapClientReader,
        orig_http_response: Response<()>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        let http_rsp = HttpAdaptedResponse::parse(icap_reader, self.http_header_size).await?;
        let final_rsp = orig_http_response.adapt_to(&http_rsp);
        state.mark_clt_send_start();
        let mut clt_send_stream = clt_send_response
            .send_response(final_rsp, false)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();

        let mut clt_body_transfer = H2StreamFromChunkedTransfer::new(
            icap_reader,
            &mut clt_send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );
        let r = self
            .do_transfer(ups_body_transfer, &mut clt_body_transfer)
            .await;
        state.record_ups_body_progress(ups_body_transfer);
        if let Err(e) = r {
            state.clt_rsp_body_size = Some(clt_body_transfer.copied_size());
            return Err(e);
        }

        state.mark_clt_send_all();
        state.clt_rsp_body_size = Some(clt_body_transfer.copied_size());
        self.icap_read_finished = clt_body_transfer.finished();
        Ok(RespmodAdaptationEndState::AdaptedTransferred(http_rsp))
    }

    async fn do_transfer<UR>(
        &self,
        mut ups_body_transfer: &mut H1BodyToChunkedTransfer<'_, UR, IcapClientWriter>,
        mut clt_body_transfer: &mut H2StreamFromChunkedTransfer<'_, IcapClientReader>,
    ) -> Result<(), H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
    {
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;
        loop {
            tokio::select! {
                r = &mut ups_body_transfer => {
                    match r {
                        Ok(_) => break,
                        Err(e) => {
                            return match e {
                                StreamCopyError::ReadFailed(e) => {
                                    Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e))
                                }
                                StreamCopyError::WriteFailed(e) => {
                                    Err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed(e))
                                }
                            };
                        }
                    }
                }
                r = &mut clt_body_transfer => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e.into()),
                    };
                }
                n = idle_interval.tick() => {
                    if ups_body_transfer.is_idle() && clt_body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if ups_body_transfer.no_cached_data() {
                                Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                            } else {
                                Err(H1ToH2RespmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        ups_body_transfer.reset_active();
                        clt_body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }

        idle_count = 0;
        loop {
            tokio::select! {
                r = &mut clt_body_transfer => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e.into()),
                    };
                }
                n = idle_interval.tick() => {
                    if clt_body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if clt_body_transfer.no_cached_data() {
                                Err(H1ToH2RespmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        clt_body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }
}
