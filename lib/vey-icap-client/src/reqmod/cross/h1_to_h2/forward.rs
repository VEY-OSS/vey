/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{IoSlice, Write};

use bytes::{BufMut, Bytes};
use h2::client::SendRequest;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};

use vey_http::{H1BodyToChunkedTransfer, HttpBodyType};
use vey_io_ext::{IdleCheck, LimitedWriteExt, StreamCopyError};

use super::{
    BidirectionalRecvHttpRequest, BidirectionalRecvIcapResponse, H1ToH2ReqmodAdaptationError,
    H1ToH2ReqmodEndState, H1ToH2ReqmodRunState, H1ToH2RequestAdapter,
};
use crate::reason::IcapErrorReason;
use crate::reqmod::IcapReqmodResponsePayload;
use crate::reqmod::h1::HttpRequestForAdaptation;
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H1ToH2RequestAdapter<I> {
    fn build_header_only_request<H>(&self, http_request: &H, http_header_len: usize) -> Vec<u8>
    where
        H: HttpRequestForAdaptation,
    {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 64);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        if self.icap_options.support_204 {
            header.put_slice(b"Allow: 204\r\n");
        }
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, null-body={http_header_len}\r\n",
        );
        http_request.append_upgrade_header(&mut header);
        header.put_slice(b"\r\n");
        header
    }

    fn build_forward_all_request(&self, http_header_len: usize) -> Vec<u8> {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 128);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, req-body={http_header_len}\r\n",
        );
        header.put_slice(b"\r\n");
        header
    }

    pub(super) async fn xfer_without_body<H, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        http_request: &H,
        ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        IW: AsyncWrite + Unpin,
    {
        let http_header = http_request.serialize_for_adapter();
        let icap_header = self.build_header_only_request(http_request, http_header.len());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([IoSlice::new(&icap_header), IoSlice::new(&http_header)])
            .await
            .map_err(H1ToH2ReqmodAdaptationError::IcapServerWriteFailed)?;
        icap_w
            .flush()
            .await
            .map_err(H1ToH2ReqmodAdaptationError::IcapServerWriteFailed)?;
        self.icap_connection.mark_writer_finished();

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
            204 => {
                if rsp.payload == IcapReqmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                }
                self.send_original_without_body(
                    state,
                    rsp,
                    http_request,
                    ups_send_req,
                    clt_informational,
                )
                .await
            }
            n if (200..300).contains(&n) => match rsp.payload {
                IcapReqmodResponsePayload::NoPayload => {
                    self.icap_connection.mark_reader_finished();
                    self.handle_icap_ok_without_payload(rsp).await
                }
                IcapReqmodResponsePayload::HttpRequestWithoutBody(header_size) => {
                    self.send_adapted_without_body(
                        state,
                        rsp,
                        header_size,
                        http_request,
                        ups_send_req,
                        clt_informational,
                    )
                    .await
                }
                IcapReqmodResponsePayload::HttpRequestWithBody(header_size) => {
                    self.send_adapted_with_body(
                        state,
                        rsp,
                        header_size,
                        http_request,
                        ups_send_req,
                        clt_informational,
                    )
                    .await
                }
                IcapReqmodResponsePayload::HttpResponseWithoutBody(header_size) => self
                    .handle_icap_http_response_without_body(rsp, header_size)
                    .await
                    .map(|rsp| H1ToH2ReqmodEndState::HttpErrResponse(rsp, None)),
                IcapReqmodResponsePayload::HttpResponseWithBody(header_size) => self
                    .handle_icap_http_response_with_body(rsp, header_size)
                    .await
                    .map(|(rsp, body)| H1ToH2ReqmodEndState::HttpErrResponse(rsp, Some(body))),
            },
            _ => {
                if rsp.payload == IcapReqmodResponsePayload::NoPayload {
                    self.icap_connection.mark_reader_finished();
                    if rsp.keep_alive {
                        self.icap_client.save_connection(self.icap_connection);
                    }
                }
                Err(H1ToH2ReqmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponse,
                    rsp.code,
                    rsp.reason,
                ))
            }
        }
    }

    pub(super) async fn xfer_without_preview<H, CR, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        http_request: &H,
        clt_body_type: HttpBodyType,
        clt_body_io: &mut CR,
        ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        CR: AsyncBufRead + Unpin,
        IW: AsyncWrite + Unpin,
    {
        let http_header = http_request.serialize_for_adapter();
        let icap_header = self.build_forward_all_request(http_header.len());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([IoSlice::new(&icap_header), IoSlice::new(&http_header)])
            .await
            .map_err(H1ToH2ReqmodAdaptationError::IcapServerWriteFailed)?;

        let mut body_transfer = H1BodyToChunkedTransfer::new(
            clt_body_io,
            &mut self.icap_connection.writer,
            clt_body_type,
            self.http_body_line_max_size,
            self.copy_config,
        );
        let bidirectional_transfer = BidirectionalRecvIcapResponse {
            icap_client: &self.icap_client,
            icap_reader: &mut self.icap_connection.reader,
            idle_checker: &self.idle_checker,
        };
        let mut rsp = bidirectional_transfer
            .transfer_and_recv(&mut body_transfer)
            .await?;
        let shared_headers = rsp.take_shared_headers();
        if !shared_headers.is_empty() {
            state.respond_shared_headers = Some(shared_headers);
        }
        if body_transfer.finished() {
            state.clt_read_finished = true;
        }

        match rsp.code {
            204 | 206 => {
                return Err(H1ToH2ReqmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::InvalidResponse,
                    rsp.code,
                    rsp.reason,
                ));
            }
            n if (200..300).contains(&n) => {}
            _ => {
                return Err(H1ToH2ReqmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponse,
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
                self.send_adapted_without_body(
                    state,
                    rsp,
                    header_size,
                    http_request,
                    ups_send_req,
                    clt_informational,
                )
                .await
            }
            IcapReqmodResponsePayload::HttpRequestWithBody(header_size) => {
                if body_transfer.finished() {
                    self.icap_connection.mark_writer_finished();
                    self.send_adapted_with_body(
                        state,
                        rsp,
                        header_size,
                        http_request,
                        ups_send_req,
                        clt_informational,
                    )
                    .await
                } else {
                    let mut bidirectional_transfer = BidirectionalRecvHttpRequest {
                        http_body_line_max_size: self.http_body_line_max_size,
                        http_trailer_max_size: self.http_trailer_max_size,
                        http_req_add_no_via_header: self.http_req_add_no_via_header,
                        http_rsp_head_recv_timeout: self.http_rsp_head_recv_timeout,
                        copy_config: self.copy_config,
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
                            ups_send_req,
                            clt_informational,
                            self.allow_continue,
                        )
                        .await?;
                    if body_transfer.finished() {
                        state.clt_read_finished = true;
                        self.icap_connection.mark_writer_finished();
                        if bidirectional_transfer.icap_read_finished {
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
                    .map(|rsp| H1ToH2ReqmodEndState::HttpErrResponse(rsp, None))
            }
            IcapReqmodResponsePayload::HttpResponseWithBody(header_size) => {
                if body_transfer.finished() {
                    self.icap_connection.mark_writer_finished();
                }
                self.handle_icap_http_response_with_body(rsp, header_size)
                    .await
                    .map(|(rsp, body)| H1ToH2ReqmodEndState::HttpErrResponse(rsp, Some(body)))
            }
        }
    }

    pub(super) async fn xfer_small_body<H, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        http_request: &H,
        clt_body: Vec<u8>,
        ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        IW: AsyncWrite + Unpin,
    {
        let http_header = http_request.serialize_for_adapter();
        let icap_header = self.build_forward_all_request(http_header.len());
        let chunk_start = format!("{:x}\r\n", clt_body.len());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([
                IoSlice::new(&icap_header),
                IoSlice::new(&http_header),
                IoSlice::new(chunk_start.as_bytes()),
                IoSlice::new(&clt_body),
                IoSlice::new(b"\r\n0\r\n\r\n"),
            ])
            .await
            .map_err(H1ToH2ReqmodAdaptationError::IcapServerWriteFailed)?;
        icap_w
            .flush()
            .await
            .map_err(H1ToH2ReqmodAdaptationError::IcapServerWriteFailed)?;
        self.icap_connection.mark_writer_finished();
        state.clt_read_finished = true;

        self.handle_small_body_response(state, http_request, ups_send_req, clt_informational)
            .await
    }

    pub(super) async fn handle_small_body_response<H, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        http_request: &H,
        ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        IW: AsyncWrite + Unpin,
    {
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
            204 | 206 => {
                return Err(H1ToH2ReqmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::InvalidResponse,
                    rsp.code,
                    rsp.reason,
                ));
            }
            n if (200..300).contains(&n) => {}
            _ => {
                return Err(H1ToH2ReqmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponse,
                    rsp.code,
                    rsp.reason,
                ));
            }
        }
        match rsp.payload {
            IcapReqmodResponsePayload::NoPayload => {
                self.icap_connection.mark_reader_finished();
                self.handle_icap_ok_without_payload(rsp).await
            }
            IcapReqmodResponsePayload::HttpRequestWithoutBody(header_size) => {
                self.send_adapted_without_body(
                    state,
                    rsp,
                    header_size,
                    http_request,
                    ups_send_req,
                    clt_informational,
                )
                .await
            }
            IcapReqmodResponsePayload::HttpRequestWithBody(header_size) => {
                self.send_adapted_with_body(
                    state,
                    rsp,
                    header_size,
                    http_request,
                    ups_send_req,
                    clt_informational,
                )
                .await
            }
            IcapReqmodResponsePayload::HttpResponseWithoutBody(header_size) => self
                .handle_icap_http_response_without_body(rsp, header_size)
                .await
                .map(|rsp| H1ToH2ReqmodEndState::HttpErrResponse(rsp, None)),
            IcapReqmodResponsePayload::HttpResponseWithBody(header_size) => self
                .handle_icap_http_response_with_body(rsp, header_size)
                .await
                .map(|(rsp, body)| H1ToH2ReqmodEndState::HttpErrResponse(rsp, Some(body))),
        }
    }
}

pub(super) fn map_stream_copy_read_write(
    e: StreamCopyError,
    read: fn(std::io::Error) -> H1ToH2ReqmodAdaptationError,
    write: fn(std::io::Error) -> H1ToH2ReqmodAdaptationError,
) -> H1ToH2ReqmodAdaptationError {
    match e {
        StreamCopyError::ReadFailed(e) => read(e),
        StreamCopyError::WriteFailed(e) => write(e),
    }
}
