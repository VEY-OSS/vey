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
    fn build_header_only_request(
        &self,
        http_req_hdr_len: usize,
        http_rsp_hdr_len: usize,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 64);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        if self.icap_options.support_204 {
            header.put_slice(b"Allow: 204\r\n");
        }
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, res-hdr={http_req_hdr_len}, null-body={}\r\n",
            http_req_hdr_len + http_rsp_hdr_len
        );
        header.put_slice(b"\r\n");
        header
    }

    fn build_forward_all_request(
        &self,
        http_req_hdr_len: usize,
        http_rsp_hdr_len: usize,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 128);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, res-hdr={http_req_hdr_len}, res-body={}\r\n",
            http_req_hdr_len + http_rsp_hdr_len
        );
        header.put_slice(b"\r\n");
        header
    }

    pub(super) async fn xfer_without_body<R, CW>(
        mut self,
        state: &mut H2ToH1RespmodRunState,
        http_request: &R,
        http_response: Response<()>,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        R: HttpRequestForAdaptation,
        CW: AsyncWrite + Unpin,
    {
        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header =
            self.build_header_only_request(http_req_header.len(), http_rsp_header.len());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([
                IoSlice::new(&icap_header),
                IoSlice::new(&http_req_header),
                IoSlice::new(&http_rsp_header),
            ])
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;
        icap_w
            .flush()
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;
        self.icap_connection.mark_writer_finished();

        let rsp = RespmodResponse::parse(
            &mut self.icap_connection.reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;

        match rsp.code {
            204 => {
                if rsp.payload == IcapRespmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                }
                self.handle_original_http_response_without_body(
                    state,
                    rsp,
                    http_response,
                    clt_writer,
                )
                .await
            }
            n if (200..300).contains(&n) => match rsp.payload {
                IcapRespmodResponsePayload::NoPayload => {
                    self.icap_connection.mark_reader_finished();
                    self.handle_icap_ok_without_payload(rsp).await
                }
                IcapRespmodResponsePayload::HttpResponseWithoutBody(header_size) => {
                    self.handle_icap_http_response_without_body(state, rsp, header_size, clt_writer)
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
            },
            _ => {
                if rsp.payload == IcapRespmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                    if rsp.keep_alive {
                        self.icap_client.save_connection(self.icap_connection);
                    }
                }
                Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponse,
                    rsp.code,
                    rsp.reason,
                ))
            }
        }
    }

    pub(super) async fn xfer_small_body<R, CW>(
        mut self,
        state: &mut H2ToH1RespmodRunState,
        http_request: &R,
        http_response: Response<()>,
        preview_data: H2PreviewData,
        ups_body: RecvStream,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        R: HttpRequestForAdaptation,
        CW: AsyncWrite + Unpin,
    {
        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header =
            self.build_forward_all_request(http_req_header.len(), http_rsp_header.len());

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
            .icap_write_all_as_chunked(icap_w)
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;
        self.recv_send_trailer(ups_body).await?;
        self.icap_connection.mark_writer_finished();
        state.mark_ups_recv_all();

        let rsp = RespmodResponse::parse(
            &mut self.icap_connection.reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;

        match rsp.code {
            204 | 206 => {
                return Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::InvalidResponse,
                    rsp.code,
                    rsp.reason,
                ));
            }
            n if (200..300).contains(&n) => {}
            _ => {
                return Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponse,
                    rsp.code,
                    rsp.reason,
                ));
            }
        }

        match rsp.payload {
            IcapRespmodResponsePayload::NoPayload => {
                self.icap_connection.mark_reader_finished();
                self.handle_icap_ok_without_payload(rsp).await
            }
            IcapRespmodResponsePayload::HttpResponseWithoutBody(header_size) => {
                self.handle_icap_http_response_without_body(state, rsp, header_size, clt_writer)
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

    async fn recv_send_trailer(
        &mut self,
        mut ups_body: RecvStream,
    ) -> Result<(), H2ToH1RespmodAdaptationError> {
        let mut trailer_transfer = H2StreamToChunkedTransfer::without_data(
            &mut ups_body,
            &mut self.icap_connection.writer,
        );
        super::client::pump_to_chunked_icap(&self.idle_checker, &mut trailer_transfer).await
    }

    pub(super) async fn xfer_without_preview<R, CW>(
        mut self,
        state: &mut H2ToH1RespmodRunState,
        http_request: &R,
        http_response: Response<()>,
        mut ups_body: RecvStream,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        R: HttpRequestForAdaptation,
        CW: AsyncWrite + Unpin,
    {
        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header =
            self.build_forward_all_request(http_req_header.len(), http_rsp_header.len());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([
                IoSlice::new(&icap_header),
                IoSlice::new(&http_req_header),
                IoSlice::new(&http_rsp_header),
            ])
            .await
            .map_err(H2ToH1RespmodAdaptationError::IcapServerWriteFailed)?;

        let mut body_transfer = H2StreamToChunkedTransfer::new(
            &mut ups_body,
            &mut self.icap_connection.writer,
            self.copy_config.yield_size(),
        );
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
                self.handle_icap_http_response_without_body(state, rsp, header_size, clt_writer)
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
}
