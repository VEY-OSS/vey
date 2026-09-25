/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::sync::Arc;

use anyhow::anyhow;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite};

use vey_http::{H1BodyToChunkedTransfer, HttpBodyDecodeReader, HttpBodyReader};
use vey_io_ext::{IdleCheck, LimitedBufReadExt, StreamCopy, StreamCopyConfig, StreamCopyError};

use super::{
    H1ReqmodAdaptationError, HttpAdaptedRequest, HttpRequestForAdaptation,
    HttpRequestUpstreamWriter, ReqmodAdaptationEndState, ReqmodAdaptationRunState,
};
use crate::reqmod::response::ReqmodResponse;
use crate::{IcapClientReader, IcapClientWriter, IcapServiceClient};

pub(super) struct BidirectionalRecvIcapResponse<'a, I: IdleCheck> {
    pub(super) icap_client: &'a Arc<IcapServiceClient>,
    pub(super) icap_reader: &'a mut IcapClientReader,
    pub(super) idle_checker: &'a I,
}

impl<I: IdleCheck> BidirectionalRecvIcapResponse<'_, I> {
    pub(super) async fn transfer_and_recv<CR>(
        self,
        state: &mut ReqmodAdaptationRunState,
        mut body_transfer: &mut H1BodyToChunkedTransfer<'_, CR, IcapClientWriter>,
    ) -> Result<ReqmodResponse, H1ReqmodAdaptationError>
    where
        CR: AsyncBufRead + Unpin,
    {
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = &mut body_transfer => {
                    return match r {
                        Ok(_) => {
                            state.clt_read_finished = true;
                            state.clt_req_body_size = Some(body_transfer.body_size());
                            self.recv_icap_response().await
                        }
                        Err(e) => {
                            state.clt_req_body_size = Some(body_transfer.body_size());
                            if body_transfer.reader_finished() {
                                state.clt_read_finished = true;
                            }
                            match e {
                                StreamCopyError::ReadFailed(e) => {
                                    Err(H1ReqmodAdaptationError::HttpClientReadFailed(e))
                                }
                                StreamCopyError::WriteFailed(e) => {
                                    Err(H1ReqmodAdaptationError::IcapServerWriteFailed(e))
                                }
                            }
                        }
                    };
                }
                r = self.icap_reader.fill_wait_data() => {
                    return match r {
                        Ok(true) => self.recv_icap_response().await,
                        Ok(false) => {
                            state.clt_req_body_size = Some(body_transfer.body_size());
                            Err(H1ReqmodAdaptationError::IcapServerConnectionClosed)
                        }
                        Err(e) => {
                            state.clt_req_body_size = Some(body_transfer.body_size());
                            Err(H1ReqmodAdaptationError::IcapServerReadFailed(e))
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;

                        let quit = self.idle_checker.check_quit(idle_count);
                        if quit {
                            state.clt_req_body_size = Some(body_transfer.body_size());
                            return if body_transfer.no_cached_data() {
                                Err(H1ReqmodAdaptationError::HttpClientReadIdle)
                            } else {
                                Err(H1ReqmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;

                        body_transfer.reset_active();
                    }

                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        state.clt_req_body_size = Some(body_transfer.body_size());
                        return Err(H1ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    async fn recv_icap_response(self) -> Result<ReqmodResponse, H1ReqmodAdaptationError> {
        let rsp = ReqmodResponse::parse(
            self.icap_reader,
            self.icap_client.config.icap_max_header_size,
            &self.icap_client.config.respond_shared_names,
        )
        .await?;
        Ok(rsp)
    }
}

pub(super) struct BidirectionalRecvHttpRequest<'a, I: IdleCheck> {
    pub(super) http_body_line_max_size: usize,
    pub(super) http_trailer_max_size: usize,
    pub(super) http_req_add_no_via_header: bool,
    pub(super) copy_config: StreamCopyConfig,
    pub(super) idle_checker: &'a I,
    pub(crate) http_header_size: usize,
    pub(crate) icap_read_finished: bool,
}

impl<I: IdleCheck> BidirectionalRecvHttpRequest<'_, I> {
    pub(super) async fn transfer<H, CR, UW>(
        &mut self,
        state: &mut ReqmodAdaptationRunState,
        clt_body_transfer: &mut H1BodyToChunkedTransfer<'_, CR, IcapClientWriter>,
        orig_http_request: &H,
        icap_reader: &mut IcapClientReader,
        ups_writer: &mut UW,
    ) -> Result<ReqmodAdaptationEndState<H>, H1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        CR: AsyncBufRead + Unpin,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        let http_req = HttpAdaptedRequest::parse(
            icap_reader,
            self.http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;
        let body_content_length = http_req.content_length;

        let final_req = orig_http_request.adapt_with_body(http_req);
        ups_writer
            .send_request_header(&final_req)
            .await
            .map_err(H1ReqmodAdaptationError::HttpUpstreamWriteFailed)?;
        state.mark_ups_send_header();

        match body_content_length {
            Some(0) => Err(H1ReqmodAdaptationError::InvalidHttpBodyFromIcapServer(
                anyhow!("Content-Length is 0 but the ICAP server response contains http-body"),
            )),
            Some(expected) => {
                let mut ups_body_reader =
                    HttpBodyDecodeReader::new_chunked(icap_reader, self.http_body_line_max_size);
                let mut ups_body_transfer =
                    StreamCopy::new(&mut ups_body_reader, ups_writer, &self.copy_config);
                let r = self
                    .do_transfer(clt_body_transfer, &mut ups_body_transfer)
                    .await;
                state.record_clt_body_progress(clt_body_transfer);
                if let Err(e) = r {
                    state.ups_req_body_size = Some(ups_body_transfer.copied_size());
                    return Err(e);
                }

                state.mark_ups_send_all();
                state.ups_req_body_size = Some(ups_body_reader.body_size());
                let copied = ups_body_reader.body_size();
                if ups_body_reader
                    .trailer(self.http_trailer_max_size)
                    .await
                    .is_ok()
                {
                    self.icap_read_finished = true;
                }

                if copied != expected {
                    return Err(H1ReqmodAdaptationError::InvalidHttpBodyFromIcapServer(
                        anyhow!("Content-Length is {expected} but decoded length is {copied}"),
                    ));
                }
                Ok(ReqmodAdaptationEndState::AdaptedTransferred(final_req))
            }
            None => {
                let mut ups_body_reader =
                    HttpBodyReader::new_chunked(icap_reader, self.http_body_line_max_size);
                let mut ups_body_transfer =
                    StreamCopy::new(&mut ups_body_reader, ups_writer, &self.copy_config);
                let r = self
                    .do_transfer(clt_body_transfer, &mut ups_body_transfer)
                    .await;
                state.record_clt_body_progress(clt_body_transfer);
                if let Err(e) = r {
                    // the chunked body is copied as on-wire bytes, so the size
                    // sent upstream is a lower bound: the payload read, less
                    // everything still buffered, as all of it could be payload
                    state.ups_req_body_size = Some(
                        ups_body_transfer
                            .reader()
                            .body_size()
                            .saturating_sub(ups_body_transfer.cached_data_size()),
                    );
                    return Err(e);
                }

                state.mark_ups_send_all();
                state.ups_req_body_size = Some(ups_body_transfer.reader().body_size());
                self.icap_read_finished = ups_body_transfer.finished();

                Ok(ReqmodAdaptationEndState::AdaptedTransferred(final_req))
            }
        }
    }

    async fn do_transfer<CR, IR, UW>(
        &self,
        mut clt_body_transfer: &mut H1BodyToChunkedTransfer<'_, CR, IcapClientWriter>,
        mut ups_body_transfer: &mut StreamCopy<'_, IR, UW>,
    ) -> Result<(), H1ReqmodAdaptationError>
    where
        CR: AsyncBufRead + Unpin,
        IR: AsyncRead + Unpin,
        UW: AsyncWrite + Unpin,
    {
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                r = &mut clt_body_transfer => {
                    match r {
                        Ok(_) => break,
                        Err(e) => {
                            return match e {
                                StreamCopyError::ReadFailed(e) => {
                                    Err(H1ReqmodAdaptationError::HttpClientReadFailed(e))
                                }
                                StreamCopyError::WriteFailed(e) => {
                                    Err(H1ReqmodAdaptationError::IcapServerWriteFailed(e))
                                }
                            };
                        }
                    }
                }
                r = &mut ups_body_transfer => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(e) => match e {
                            StreamCopyError::ReadFailed(e) => {
                                Err(H1ReqmodAdaptationError::IcapServerReadFailed(e))
                            }
                            StreamCopyError::WriteFailed(e) => {
                                Err(H1ReqmodAdaptationError::HttpUpstreamWriteFailed(e))
                            }
                        },
                    };
                }
                n = idle_interval.tick() => {
                    if clt_body_transfer.is_idle() && ups_body_transfer.is_idle() {
                        idle_count += n;

                        let quit = self.idle_checker.check_quit(idle_count);
                        if quit {
                            return if clt_body_transfer.no_cached_data() {
                                Err(H1ReqmodAdaptationError::HttpClientReadIdle)
                            } else {
                                Err(H1ReqmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;

                        clt_body_transfer.reset_active();
                        ups_body_transfer.reset_active();
                    }

                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }

        idle_count = 0;
        loop {
            tokio::select! {
                r = &mut ups_body_transfer => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(e) => match e {
                            StreamCopyError::ReadFailed(e) => {
                                Err(H1ReqmodAdaptationError::IcapServerReadFailed(e))
                            }
                            StreamCopyError::WriteFailed(e) => {
                                Err(H1ReqmodAdaptationError::HttpUpstreamWriteFailed(e))
                            }
                        },
                    };
                }
                n = idle_interval.tick() => {
                    if ups_body_transfer.is_idle() {
                        idle_count += n;

                        let quit = self.idle_checker.check_quit(idle_count);
                        if quit {
                            return if ups_body_transfer.no_cached_data() {
                                Err(H1ReqmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H1ReqmodAdaptationError::HttpUpstreamWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;

                        ups_body_transfer.reset_active();
                    }

                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }
}
