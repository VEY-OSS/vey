/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{IoSlice, Write};

use bytes::BufMut;
use http::{Request, Response};
use tokio::io::{AsyncBufRead, AsyncWriteExt};

use vey_h2::{RequestExt, ResponseExt};
use vey_http::{H1BodyToChunkedTransfer, HttpBodyReader, HttpBodyType};
use vey_io_ext::{IdleCheck, LimitedWriteExt, StreamCopy, StreamCopyError};

use super::{
    BidirectionalRecvHttpResponse, BidirectionalRecvIcapResponse, H1ToH2RespmodAdaptationError,
    H1ToH2ResponseAdapter, H2SendResponseToClient, RespmodAdaptationEndState,
    RespmodAdaptationRunState,
};
use crate::reason::IcapErrorReason;
use crate::respmod::IcapRespmodResponsePayload;
use crate::respmod::response::RespmodResponse;

impl<I: IdleCheck> H1ToH2ResponseAdapter<I> {
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

    pub(super) async fn xfer_small_body<CW>(
        mut self,
        state: &mut RespmodAdaptationRunState,
        http_request: &Request<()>,
        http_response: Response<()>,
        ups_body: Vec<u8>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        CW: H2SendResponseToClient,
    {
        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header =
            self.build_forward_all_request(http_req_header.len(), http_rsp_header.len());

        let icap_w = &mut self.icap_connection.writer;
        if ups_body.is_empty() {
            icap_w
                .write_all_vectored([
                    IoSlice::new(&icap_header),
                    IoSlice::new(&http_req_header),
                    IoSlice::new(&http_rsp_header),
                    IoSlice::new(b"0\r\n\r\n"),
                ])
                .await
                .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;
        } else {
            let chunk_start = format!("{:x}\r\n", ups_body.len());
            icap_w
                .write_all_vectored([
                    IoSlice::new(&icap_header),
                    IoSlice::new(&http_req_header),
                    IoSlice::new(&http_rsp_header),
                    IoSlice::new(chunk_start.as_bytes()),
                    IoSlice::new(&ups_body),
                    IoSlice::new(b"\r\n0\r\n\r\n"),
                ])
                .await
                .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;
        }
        icap_w
            .flush()
            .await
            .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;
        self.icap_connection.mark_writer_finished();
        state.mark_ups_recv_all();
        state.ups_rsp_body_size = Some(ups_body.len() as u64);

        self.handle_small_body_response(state, http_response, clt_send_response)
            .await
    }

    pub(super) async fn xfer_small_body_chunked<UR, CW>(
        mut self,
        state: &mut RespmodAdaptationRunState,
        http_request: &Request<()>,
        http_response: Response<()>,
        ups_body: Vec<u8>,
        mut trailer_reader: HttpBodyReader<'_, UR>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header =
            self.build_forward_all_request(http_req_header.len(), http_rsp_header.len());

        if ups_body.is_empty() {
            self.icap_connection
                .writer
                .write_all_vectored([
                    IoSlice::new(&icap_header),
                    IoSlice::new(&http_req_header),
                    IoSlice::new(&http_rsp_header),
                    IoSlice::new(b"0\r\n"),
                ])
                .await
                .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;
        } else {
            let chunk_start = format!("{:x}\r\n", ups_body.len());
            self.icap_connection
                .writer
                .write_all_vectored([
                    IoSlice::new(&icap_header),
                    IoSlice::new(&http_req_header),
                    IoSlice::new(&http_rsp_header),
                    IoSlice::new(chunk_start.as_bytes()),
                    IoSlice::new(&ups_body),
                    IoSlice::new(b"\r\n0\r\n"),
                ])
                .await
                .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;
        }

        self.recv_send_trailer(&mut trailer_reader).await?;
        state.mark_ups_recv_all();
        state.ups_rsp_body_size = Some(ups_body.len() as u64);
        self.icap_connection.mark_writer_finished();

        self.handle_small_body_response(state, http_response, clt_send_response)
            .await
    }

    async fn recv_send_trailer<UR>(
        &mut self,
        trailer_reader: &mut HttpBodyReader<'_, UR>,
    ) -> Result<(), H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
    {
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;
        let mut trailer_transfer = StreamCopy::new(
            trailer_reader,
            &mut self.icap_connection.writer,
            &self.copy_config,
        );
        loop {
            tokio::select! {
                biased;
                r = &mut trailer_transfer => {
                    return match r {
                        Ok(_) => Ok(()),
                        Err(StreamCopyError::ReadFailed(e)) => {
                            Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => {
                            Err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed(e))
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if trailer_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if trailer_transfer.no_cached_data() {
                                Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                            } else {
                                Err(H1ToH2RespmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        trailer_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    pub(super) async fn handle_small_body_response<CW>(
        mut self,
        state: &mut RespmodAdaptationRunState,
        orig_http_response: Response<()>,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        CW: H2SendResponseToClient,
    {
        let rsp = RespmodResponse::parse(
            &mut self.icap_connection.reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;
        match rsp.code {
            204 | 206 => {
                return Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::InvalidResponse,
                    rsp.code,
                    rsp.reason,
                ));
            }
            n if (200..300).contains(&n) => {}
            _ => {
                return Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
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
                self.handle_icap_http_response_without_body(
                    state,
                    rsp,
                    header_size,
                    orig_http_response,
                    clt_send_response,
                )
                .await
            }
            IcapRespmodResponsePayload::HttpResponseWithBody(header_size) => {
                self.handle_icap_http_response_with_body_after_transfer(
                    state,
                    rsp,
                    header_size,
                    orig_http_response,
                    clt_send_response,
                )
                .await
            }
        }
    }

    pub(super) async fn xfer_without_preview<UR, CW>(
        mut self,
        state: &mut RespmodAdaptationRunState,
        http_request: &Request<()>,
        http_response: Response<()>,
        ups_body_type: HttpBodyType,
        ups_body_io: &mut UR,
        clt_send_response: &mut CW,
    ) -> Result<RespmodAdaptationEndState, H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
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
            .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;

        let mut body_transfer = H1BodyToChunkedTransfer::new(
            ups_body_io,
            &mut self.icap_connection.writer,
            ups_body_type,
            self.http_body_line_max_size,
            self.copy_config,
        );
        let bidirectional_transfer = BidirectionalRecvIcapResponse {
            icap_client: &self.icap_client,
            icap_reader: &mut self.icap_connection.reader,
            idle_checker: &self.idle_checker,
        };
        let rsp = bidirectional_transfer
            .transfer_and_recv(state, &mut body_transfer)
            .await?;
        if body_transfer.finished() {
            state.mark_ups_recv_all();
            state.ups_rsp_body_size = Some(body_transfer.body_size());
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
                    http_response,
                    clt_send_response,
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
                        http_response,
                        clt_send_response,
                    )
                    .await
                } else {
                    let mut bidirectional_transfer = BidirectionalRecvHttpResponse {
                        http_body_line_max_size: self.http_body_line_max_size,
                        http_trailer_max_size: self.http_trailer_max_size,
                        copy_config: self.copy_config,
                        idle_checker: &self.idle_checker,
                        http_header_size: header_size,
                        icap_read_finished: false,
                    };
                    let r = bidirectional_transfer
                        .transfer(
                            state,
                            &mut body_transfer,
                            &mut self.icap_connection.reader,
                            http_response,
                            clt_send_response,
                        )
                        .await?;
                    if body_transfer.finished() {
                        state.mark_ups_recv_all();
                        state.ups_rsp_body_size = Some(body_transfer.body_size());
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
        }
    }
}
