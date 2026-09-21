/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{IoSlice, Write};

use bytes::BufMut;
use h2::RecvStream;
use tokio::io::AsyncWriteExt;

use vey_h2::{H2PreviewData, H2StreamToChunkedTransfer};
use vey_io_ext::{IdleCheck, LimitedWriteExt};

use super::{
    BidirectionalRecvHttpRequest, BidirectionalRecvIcapResponse, H2ToH1ReqmodAdaptationError,
    H2ToH1RequestAdapter, HttpRequestForAdaptation, HttpRequestUpstreamWriter,
    ReqmodAdaptationEndState, ReqmodAdaptationRunState,
};
use crate::reason::IcapErrorReason;
use crate::reqmod::IcapReqmodResponsePayload;
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H2ToH1RequestAdapter<I> {
    fn build_preview_request(&self, http_header_len: usize, preview_size: usize) -> Vec<u8> {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 128);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        // do not send `Allow: 204, 206` as we don't want to accept 204/206 after 100-continue
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, req-body={http_header_len}\r\nPreview: {preview_size}\r\n",
        );
        header.put_slice(b"\r\n");
        header
    }

    pub(super) async fn xfer_with_preview<H, UW>(
        mut self,
        state: &mut ReqmodAdaptationRunState,
        http_request: &H,
        clt_body: &mut RecvStream,
        ups_writer: &mut UW,
        max_preview_size: usize,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        UW: HttpRequestUpstreamWriter<H> + Unpin,
    {
        let mut preview_data = H2PreviewData::new(max_preview_size);
        preview_data.recv_all(clt_body, &self.idle_checker).await?;

        if preview_data.end_of_data() {
            return self
                .xfer_small_body(state, http_request, preview_data, clt_body, ups_writer)
                .await;
        }

        let http_header = http_request.serialize_for_adapter();
        let icap_header =
            self.build_preview_request(http_header.len(), preview_data.preview_size());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([IoSlice::new(&icap_header), IoSlice::new(&http_header)])
            .await
            .map_err(H2ToH1ReqmodAdaptationError::IcapServerWriteFailed)?;
        preview_data
            .icap_write_preview_data(icap_w)
            .await
            .map_err(H2ToH1ReqmodAdaptationError::IcapServerWriteFailed)?;
        icap_w
            .flush()
            .await
            .map_err(H2ToH1ReqmodAdaptationError::IcapServerWriteFailed)?;

        let mut rsp = ReqmodResponse::parse(
            &mut self.icap_connection.reader,
            self.icap_client.config.icap_max_header_size,
            &self.icap_client.config.respond_shared_names,
        )
        .await?;
        let shared_headers = rsp.take_shared_headers();
        if !shared_headers.is_empty() {
            state.respond_shared_headers = Some(shared_headers);
        }

        match rsp.code {
            100 => {
                let preview_hdr = preview_data.preview_size() as u64;
                let mut body_transfer = if let Some(left_data) = preview_data.take_left() {
                    H2StreamToChunkedTransfer::with_chunk(
                        clt_body,
                        &mut self.icap_connection.writer,
                        self.copy_config.yield_size(),
                        left_data,
                    )
                } else {
                    H2StreamToChunkedTransfer::new(
                        clt_body,
                        &mut self.icap_connection.writer,
                        self.copy_config.yield_size(),
                    )
                };
                body_transfer.add_copied(preview_hdr);

                let bidirectional_transfer = BidirectionalRecvIcapResponse {
                    icap_client: &self.icap_client,
                    icap_reader: &mut self.icap_connection.reader,
                    idle_checker: &self.idle_checker,
                };
                let rsp = bidirectional_transfer
                    .transfer_and_recv(state, &mut body_transfer)
                    .await?;
                if body_transfer.finished() {
                    state.clt_req_body_size = Some(body_transfer.copied_size());
                }

                match rsp.code {
                    204 | 206 => {
                        return Err(H2ToH1ReqmodAdaptationError::IcapServerErrorResponse(
                            IcapErrorReason::InvalidResponseAfterContinue,
                            rsp.code,
                            rsp.reason,
                        ));
                    }
                    n if (200..300).contains(&n) => {}
                    _ => {
                        return Err(H2ToH1ReqmodAdaptationError::IcapServerErrorResponse(
                            IcapErrorReason::UnknownResponseAfterContinue,
                            rsp.code,
                            rsp.reason,
                        ));
                    }
                }
                match rsp.payload {
                    IcapReqmodResponsePayload::NoPayload => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                        }
                        self.icap_connection.mark_reader_finished();
                        self.handle_icap_ok_without_payload(rsp).await
                    }
                    IcapReqmodResponsePayload::HttpRequestWithoutBody(header_size) => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                        }
                        self.handle_icap_http_request_without_body(
                            state,
                            rsp,
                            header_size,
                            http_request,
                            ups_writer,
                        )
                        .await
                    }
                    IcapReqmodResponsePayload::HttpRequestWithBody(header_size) => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                            self.handle_icap_http_request_with_body_after_transfer(
                                state,
                                rsp,
                                header_size,
                                http_request,
                                ups_writer,
                            )
                            .await
                        } else {
                            let mut bidirectional_transfer = BidirectionalRecvHttpRequest {
                                copy_config: self.copy_config,
                                http_body_line_max_size: self.http_body_line_max_size,
                                http_req_add_no_via_header: self.http_req_add_no_via_header,
                                idle_checker: &self.idle_checker,
                                http_header_size: header_size,
                                icap_read_finished: false,
                            };
                            let r = bidirectional_transfer
                                .transfer(
                                    state,
                                    &mut body_transfer,
                                    http_request,
                                    &mut self.icap_connection.reader,
                                    ups_writer,
                                )
                                .await?;
                            let icap_read_finished = bidirectional_transfer.icap_read_finished;
                            if body_transfer.finished() {
                                state.clt_req_body_size = Some(body_transfer.copied_size());
                                self.icap_connection.mark_writer_finished();
                                if icap_read_finished {
                                    self.icap_connection.mark_reader_finished();
                                    if rsp.keep_alive {
                                        self.icap_client.save_connection(self.icap_connection);
                                    }
                                }
                            }
                            Ok(r)
                        }
                    }
                    IcapReqmodResponsePayload::HttpResponseWithoutBody(header_size) => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                        }
                        self.handle_icap_http_response_without_body(rsp, header_size)
                            .await
                            .map(|rsp| ReqmodAdaptationEndState::HttpErrResponse(rsp, None))
                    }
                    IcapReqmodResponsePayload::HttpResponseWithBody(header_size) => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                        }
                        self.handle_icap_http_response_with_body(rsp, header_size)
                            .await
                            .map(|(rsp, body)| {
                                ReqmodAdaptationEndState::HttpErrResponse(rsp, Some(body))
                            })
                    }
                }
            }
            204 => {
                self.icap_connection.mark_writer_finished();
                if rsp.payload == IcapReqmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                }
                self.handle_original_http_request_with_body(
                    state,
                    rsp,
                    http_request,
                    preview_data,
                    clt_body,
                    ups_writer,
                )
                .await
            }
            206 => Err(H2ToH1ReqmodAdaptationError::NotImplemented(
                "ICAP-REQMOD-206",
            )),
            n if (200..300).contains(&n) => {
                self.icap_connection.mark_writer_finished();
                match rsp.payload {
                    IcapReqmodResponsePayload::NoPayload => {
                        self.icap_connection.mark_reader_finished();
                        self.handle_icap_ok_without_payload(rsp).await
                    }
                    IcapReqmodResponsePayload::HttpRequestWithoutBody(header_size) => {
                        self.handle_icap_http_request_without_body(
                            state,
                            rsp,
                            header_size,
                            http_request,
                            ups_writer,
                        )
                        .await
                    }
                    IcapReqmodResponsePayload::HttpRequestWithBody(header_size) => {
                        self.handle_icap_http_request_with_body_after_transfer(
                            state,
                            rsp,
                            header_size,
                            http_request,
                            ups_writer,
                        )
                        .await
                    }
                    IcapReqmodResponsePayload::HttpResponseWithoutBody(header_size) => self
                        .handle_icap_http_response_without_body(rsp, header_size)
                        .await
                        .map(|rsp| ReqmodAdaptationEndState::HttpErrResponse(rsp, None)),
                    IcapReqmodResponsePayload::HttpResponseWithBody(header_size) => self
                        .handle_icap_http_response_with_body(rsp, header_size)
                        .await
                        .map(|(rsp, body)| {
                            ReqmodAdaptationEndState::HttpErrResponse(rsp, Some(body))
                        }),
                }
            }
            _ => {
                self.icap_connection.mark_writer_finished();
                if rsp.payload == IcapReqmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                    if rsp.keep_alive {
                        self.icap_client.save_connection(self.icap_connection);
                    }
                }
                Err(H2ToH1ReqmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponseForPreview,
                    rsp.code,
                    rsp.reason,
                ))
            }
        }
    }
}
