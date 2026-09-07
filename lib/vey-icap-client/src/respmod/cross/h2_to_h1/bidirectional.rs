/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_h2::H2StreamToChunkedTransfer;
use vey_http::HttpBodyReader;
use vey_http::client::HttpAdaptedResponse;
use vey_io_ext::{IdleCheck, LimitedBufReadExt, StreamCopy, StreamCopyConfig, StreamCopyError};

use super::super::serialize_adapted_response_as_h1_chunked;
use super::client::map_to_chunked_icap_err;
use super::{H2ToH1RespmodAdaptationError, H2ToH1RespmodEndState, H2ToH1RespmodRunState};
use crate::reason::IcapErrorReason;
use crate::respmod::response::RespmodResponse;
use crate::{IcapClientReader, IcapClientWriter, IcapServiceClient};

pub(super) struct BidirectionalRecvIcapResponse<'a, I: IdleCheck> {
    pub(super) icap_client: &'a Arc<IcapServiceClient>,
    pub(super) icap_reader: &'a mut IcapClientReader,
    pub(super) idle_checker: &'a I,
}

impl<I: IdleCheck> BidirectionalRecvIcapResponse<'_, I> {
    pub(super) async fn transfer_and_recv(
        self,
        mut body_transfer: &mut H2StreamToChunkedTransfer<'_, IcapClientWriter>,
    ) -> Result<RespmodResponse, H2ToH1RespmodAdaptationError> {
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = &mut body_transfer => {
                    return match r {
                        Ok(_) => self.recv_icap_response().await,
                        Err(e) => Err(map_to_chunked_icap_err(e)),
                    };
                }
                r = self.icap_reader.fill_wait_data() => {
                    return match r {
                        Ok(true) => self.recv_icap_response().await,
                        Ok(false) => Err(H2ToH1RespmodAdaptationError::IcapServerConnectionClosed),
                        Err(e) => Err(H2ToH1RespmodAdaptationError::IcapServerReadFailed(e)),
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if body_transfer.no_cached_data() {
                                Err(H2ToH1RespmodAdaptationError::HttpUpstreamReadIdle)
                            } else {
                                Err(H2ToH1RespmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H2ToH1RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    pub(super) async fn recv_icap_response(
        self,
    ) -> Result<RespmodResponse, H2ToH1RespmodAdaptationError> {
        let rsp = RespmodResponse::parse(
            self.icap_reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;

        match rsp.code {
            204 | 206 => Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
                IcapErrorReason::InvalidResponseAfterContinue,
                rsp.code,
                rsp.reason,
            )),
            n if (200..300).contains(&n) => Ok(rsp),
            _ => Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
                IcapErrorReason::UnknownResponseAfterContinue,
                rsp.code,
                rsp.reason,
            )),
        }
    }
}

pub(super) struct BidirectionalRecvHttpResponse<'a, I: IdleCheck> {
    pub(super) icap_reader: &'a mut IcapClientReader,
    pub(super) copy_config: StreamCopyConfig,
    pub(super) http_body_line_max_size: usize,
    pub(super) idle_checker: &'a I,
    pub(super) http_header_size: usize,
    pub(super) icap_read_finished: bool,
}

impl<I: IdleCheck> BidirectionalRecvHttpResponse<'_, I> {
    pub(super) async fn transfer<CW>(
        &mut self,
        state: &mut H2ToH1RespmodRunState,
        ups_body_transfer: &mut H2StreamToChunkedTransfer<'_, IcapClientWriter>,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        CW: AsyncWrite + Unpin,
    {
        let http_header_size = self.http_header_size;
        let http_body_line_max_size = self.http_body_line_max_size;
        let copy_config = self.copy_config;
        let idle_checker = self.idle_checker;

        let http_rsp = HttpAdaptedResponse::parse(self.icap_reader, http_header_size).await?;
        let head = serialize_adapted_response_as_h1_chunked(&http_rsp, true);
        state.mark_clt_send_start();
        clt_writer
            .write_all(&head)
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        state.mark_clt_send_header();

        let mut clt_body_reader =
            HttpBodyReader::new_chunked(self.icap_reader, http_body_line_max_size);
        let mut clt_body_transfer = StreamCopy::new(&mut clt_body_reader, clt_writer, &copy_config);
        Self::do_transfer(idle_checker, ups_body_transfer, &mut clt_body_transfer).await?;

        state.mark_clt_send_all();
        self.icap_read_finished = clt_body_transfer.finished();
        Ok(H2ToH1RespmodEndState::AdaptedTransferred(http_rsp))
    }

    async fn do_transfer<IR, CW>(
        idle_checker: &I,
        mut ups_body_transfer: &mut H2StreamToChunkedTransfer<'_, IcapClientWriter>,
        mut clt_body_transfer: &mut StreamCopy<'_, IR, CW>,
    ) -> Result<(), H2ToH1RespmodAdaptationError>
    where
        IR: AsyncRead + Unpin,
        CW: AsyncWrite + Unpin,
    {
        let mut idle_interval = idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                r = &mut ups_body_transfer => {
                    return match r {
                        Ok(_) => {
                            match clt_body_transfer.await {
                                Ok(_) => Ok(()),
                                Err(StreamCopyError::ReadFailed(e)) => {
                                    Err(H2ToH1RespmodAdaptationError::IcapServerReadFailed(e))
                                }
                                Err(StreamCopyError::WriteFailed(e)) => {
                                    Err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed(e))
                                }
                            }
                        }
                        Err(e) => Err(map_to_chunked_icap_err(e)),
                    };
                }
                r = &mut clt_body_transfer => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(StreamCopyError::ReadFailed(e)) => {
                            Err(H2ToH1RespmodAdaptationError::IcapServerReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => {
                            Err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed(e))
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if ups_body_transfer.is_idle() && clt_body_transfer.is_idle() {
                        idle_count += n;
                        if idle_checker.check_quit(idle_count) {
                            return if ups_body_transfer.is_idle() {
                                if ups_body_transfer.no_cached_data() {
                                    Err(H2ToH1RespmodAdaptationError::HttpUpstreamReadIdle)
                                } else {
                                    Err(H2ToH1RespmodAdaptationError::IcapServerWriteIdle)
                                }
                            } else if clt_body_transfer.no_cached_data() {
                                Err(H2ToH1RespmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H2ToH1RespmodAdaptationError::HttpClientWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        ups_body_transfer.reset_active();
                        clt_body_transfer.reset_active();
                    }
                    if let Some(reason) = idle_checker.check_force_quit() {
                        return Err(H2ToH1RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }
}
