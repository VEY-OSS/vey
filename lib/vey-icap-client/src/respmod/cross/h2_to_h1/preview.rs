/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{IoSlice, Write};

use bytes::BufMut;
use h2::RecvStream;
use http::Response;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use vey_h2::{H2PreviewData, H2StreamToChunkedTransfer, ResponseExt};
use vey_io_ext::{IdleCheck, LimitedWriteExt};

use super::{
    BidirectionalRecvHttpResponse, BidirectionalRecvIcapResponse, H2ToH1RespmodAdaptationError,
    H2ToH1RespmodEndState, H2ToH1RespmodRunState, H2ToH1ResponseAdapter,
};
use crate::reason::IcapErrorReason;
use crate::reqmod::h1::HttpRequestForAdaptation;
use crate::respmod::IcapRespmodResponsePayload;
use crate::respmod::response::RespmodResponse;

impl<I: IdleCheck> H2ToH1ResponseAdapter<I> {
    fn build_preview_request(
        &self,
        http_req_hdr_len: usize,
        http_rsp_hdr_len: usize,
        preview_size: usize,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 128);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, res-hdr={http_req_hdr_len}, res-body={}\r\nPreview: {preview_size}\r\n",
            http_req_hdr_len + http_rsp_hdr_len,
        );
        header.put_slice(b"\r\n");
        header
    }

    pub(super) async fn xfer_with_preview<R, CW>(
        mut self,
        state: &mut H2ToH1RespmodRunState,
        http_request: &R,
        http_response: Response<()>,
        mut ups_body: RecvStream,
        clt_writer: &mut CW,
        max_preview_size: usize,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        R: HttpRequestForAdaptation,
        CW: AsyncWrite + Unpin,
    {
        let mut preview_data = H2PreviewData::new(max_preview_size);
        preview_data
            .recv_initial(
                &mut ups_body,
                self.icap_client.config.preview_data_read_timeout,
            )
            .await?;

        if preview_data.end_of_data() {
            return self
                .xfer_small_body(
                    state,
                    http_request,
                    http_response,
                    preview_data,
                    ups_body,
                    clt_writer,
                )
                .await;
        } else if preview_data.preview_size() == 0 {
            return self
                .xfer_without_preview(state, http_request, http_response, ups_body, clt_writer)
                .await;
        }

        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header = self.build_preview_request(
            http_req_header.len(),
            http_rsp_header.len(),
            preview_data.preview_size(),
        );

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([
                IoSlice::new(&icap_header),
                IoSlice::new(&http_req_header),
                IoSlice::new(&http_rsp_header),
            ])
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;
        preview_data
            .icap_write_preview_data(icap_w)
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;
        icap_w
            .flush()
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;

        let rsp = RespmodResponse::parse(
            &mut self.icap_connection.reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;

        match rsp.code {
            100 => {
                let mut body_transfer = if let Some(left_data) = preview_data.take_left() {
                    H2StreamToChunkedTransfer::with_chunk(
                        &mut ups_body,
                        &mut self.icap_connection.writer,
                        self.copy_config.yield_size(),
                        left_data,
                    )
                } else {
                    H2StreamToChunkedTransfer::new(
                        &mut ups_body,
                        &mut self.icap_connection.writer,
                        self.copy_config.yield_size(),
                    )
                };

                let bidirectional_transfer = BidirectionalRecvIcapResponse {
                    icap_client: &self.icap_client,
                    icap_reader: &mut self.icap_connection.reader,
                    idle_checker: &self.idle_checker,
                };
                let rsp = bidirectional_transfer
                    .transfer_and_recv(&mut body_transfer)
                    .await?;
                if body_transfer.finished() {
                    state.mark_ups_recv_all();
                }

                match rsp.payload {
                    IcapRespmodResponsePayload::NoPayload => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                        }
                        self.icap_connection.mark_reader_finished();
                        self.handle_icap_ok_without_payload(rsp).await
                    }
                    IcapRespmodResponsePayload::HttpResponseWithoutBody(header_size) => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                        }
                        self.handle_icap_http_response_without_body(
                            state,
                            rsp,
                            header_size,
                            clt_writer,
                        )
                        .await
                    }
                    IcapRespmodResponsePayload::HttpResponseWithBody(header_size) => {
                        if body_transfer.finished() {
                            self.icap_connection.mark_writer_finished();
                            self.handle_icap_http_response_with_body_after_transfer(
                                state,
                                rsp,
                                header_size,
                                clt_writer,
                            )
                            .await
                        } else {
                            let mut bidirectional_transfer = BidirectionalRecvHttpResponse {
                                icap_reader: &mut self.icap_connection.reader,
                                copy_config: self.copy_config,
                                http_body_line_max_size: self.http_body_line_max_size,
                                idle_checker: &self.idle_checker,
                                http_header_size: header_size,
                                icap_read_finished: false,
                            };
                            let r = bidirectional_transfer
                                .transfer(state, &mut body_transfer, clt_writer)
                                .await?;
                            let icap_read_finished = bidirectional_transfer.icap_read_finished;
                            if body_transfer.finished() {
                                state.mark_ups_recv_all();
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
                }
            }
            204 => {
                self.icap_connection.mark_writer_finished();
                if rsp.payload == IcapRespmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                }
                self.handle_original_http_response_with_body(
                    state,
                    rsp,
                    http_response,
                    preview_data,
                    ups_body,
                    clt_writer,
                )
                .await
            }
            206 => Err(H2ToH1RespmodAdaptationError::NotImplemented(
                "ICAP-RESPMOD-206",
            )),
            n if (200..300).contains(&n) => {
                self.icap_connection.mark_writer_finished();
                match rsp.payload {
                    IcapRespmodResponsePayload::NoPayload => {
                        self.icap_connection.mark_reader_finished();
                        self.handle_icap_ok_without_payload(rsp).await
                    }
                    IcapRespmodResponsePayload::HttpResponseWithoutBody(header_size) => {
                        self.handle_icap_http_response_without_body(
                            state,
                            rsp,
                            header_size,
                            clt_writer,
                        )
                        .await
                    }
                    IcapRespmodResponsePayload::HttpResponseWithBody(header_size) => {
                        self.handle_icap_http_response_with_body_after_transfer(
                            state,
                            rsp,
                            header_size,
                            clt_writer,
                        )
                        .await
                    }
                }
            }
            _ => {
                self.icap_connection.mark_writer_finished();
                if rsp.payload == IcapRespmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                    if rsp.keep_alive {
                        self.icap_client.save_connection(self.icap_connection);
                    }
                }
                Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponseForPreview,
                    rsp.code,
                    rsp.reason,
                ))
            }
        }
    }
}
