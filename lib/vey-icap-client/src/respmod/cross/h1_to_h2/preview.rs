/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::future::poll_fn;
use std::io::{IoSlice, Write};
use std::pin::Pin;
use std::task::Poll;
use std::time::Duration;

use bytes::BufMut;
use http::Request;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, AsyncWriteExt, ReadBuf};

use vey_h2::RequestExt;
use vey_http::{ChunkedDataDecodeReader, H1BodyToChunkedTransfer, HttpBodyReader, HttpBodyType};
use vey_io_ext::{IdleCheck, LimitedWriteExt};

use super::{
    BidirectionalRecvHttpResponse, BidirectionalRecvIcapResponse, H1ToH2RespmodAdaptationError,
    H1ToH2RespmodEndState, H1ToH2RespmodRunState, H1ToH2ResponseAdapter,
};
use crate::reason::IcapErrorReason;
use crate::respmod::IcapRespmodResponsePayload;
use crate::respmod::h1::HttpResponseForAdaptation;
use crate::respmod::h2::H2SendResponseToClient;
use crate::respmod::response::RespmodResponse;

impl<I: IdleCheck> H1ToH2ResponseAdapter<I> {
    fn build_preview_request(
        &self,
        http_req_hdr_len: usize,
        http_rsp_hdr_len: usize,
        preview_size: usize,
    ) -> Vec<u8> {
        let mut header = Vec::with_capacity(self.icap_client.partial_request_header.len() + 128);
        header.extend_from_slice(&self.icap_client.partial_request_header);
        self.push_extended_headers(&mut header);
        let body_offset = http_req_hdr_len + http_rsp_hdr_len;
        let _ = write!(
            header,
            "Encapsulated: req-hdr=0, res-hdr={http_req_hdr_len}, res-body={body_offset}\r\nPreview: {preview_size}\r\n",
        );
        header.put_slice(b"\r\n");
        header
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn xfer_with_preview<H, UR, CW>(
        mut self,
        state: &mut H1ToH2RespmodRunState,
        http_request: &Request<()>,
        http_response: &H,
        ups_body_type: HttpBodyType,
        ups_body_io: &mut UR,
        clt_send_response: &mut CW,
        preview_size: usize,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        let mut left_chunk_size = 0;
        let preview_buf: Vec<u8>;
        let ups_body_type = match ups_body_type {
            HttpBodyType::ReadUntilEnd => {
                let mut ups_body_reader = HttpBodyReader::new_read_until_end(ups_body_io);
                match self
                    .read_plain_preview_data(
                        &mut ups_body_reader,
                        preview_size,
                        self.icap_client.config.preview_data_read_timeout,
                    )
                    .await
                {
                    Ok(Some(buf)) => preview_buf = buf,
                    Ok(None) => {
                        return self
                            .xfer_without_preview(
                                state,
                                http_request,
                                http_response,
                                ups_body_type,
                                ups_body_io,
                                clt_send_response,
                            )
                            .await;
                    }
                    Err(e) => return Err(e),
                }
                if ups_body_reader.finished() {
                    if preview_buf.is_empty() {
                        state.mark_ups_recv_no_body();
                        return self
                            .xfer_without_body(
                                state,
                                http_request,
                                http_response,
                                clt_send_response,
                            )
                            .await;
                    } else {
                        state.mark_ups_recv_all();
                        return self
                            .xfer_small_body(
                                state,
                                http_request,
                                http_response,
                                preview_buf,
                                clt_send_response,
                            )
                            .await;
                    }
                }
                HttpBodyType::ReadUntilEnd
            }
            HttpBodyType::ContentLength(n) => {
                let mut ups_body_reader = HttpBodyReader::new_fixed_length(ups_body_io, n);
                match self
                    .read_plain_preview_data(
                        &mut ups_body_reader,
                        preview_size,
                        self.icap_client.config.preview_data_read_timeout,
                    )
                    .await
                {
                    Ok(Some(buf)) => preview_buf = buf,
                    Ok(None) => {
                        return self
                            .xfer_without_preview(
                                state,
                                http_request,
                                http_response,
                                ups_body_type,
                                ups_body_io,
                                clt_send_response,
                            )
                            .await;
                    }
                    Err(e) => return Err(e),
                }
                if ups_body_reader.finished() {
                    state.mark_ups_recv_all();
                    return self
                        .xfer_small_body(
                            state,
                            http_request,
                            http_response,
                            preview_buf,
                            clt_send_response,
                        )
                        .await;
                }
                HttpBodyType::ContentLength(n - (preview_buf.len() as u64))
            }
            HttpBodyType::Chunked => {
                let mut ups_body_reader =
                    ChunkedDataDecodeReader::new(ups_body_io, self.http_body_line_max_size);
                match self
                    .read_chunked_preview_data(
                        &mut ups_body_reader,
                        preview_size,
                        self.icap_client.config.preview_data_read_timeout,
                    )
                    .await
                {
                    Ok(Some(buf)) => preview_buf = buf,
                    Ok(None) => {
                        return self
                            .xfer_without_preview(
                                state,
                                http_request,
                                http_response,
                                ups_body_type,
                                ups_body_io,
                                clt_send_response,
                            )
                            .await;
                    }
                    Err(e) => return Err(e),
                }
                if ups_body_reader.finished() {
                    let trailer_reader =
                        HttpBodyReader::new_trailer(ups_body_io, self.http_body_line_max_size);
                    return self
                        .xfer_small_body_chunked(
                            state,
                            http_request,
                            http_response,
                            preview_buf,
                            trailer_reader,
                            clt_send_response,
                        )
                        .await;
                }
                left_chunk_size = ups_body_reader.left_chunk_size().ok_or_else(|| {
                    H1ToH2RespmodAdaptationError::InternalServerError(
                        "broken chunked encoding after preview read",
                    )
                })?;
                HttpBodyType::Chunked
            }
        };

        self.send_preview_data(http_request, http_response, &preview_buf)
            .await?;

        let rsp = RespmodResponse::parse(
            &mut self.icap_connection.reader,
            self.icap_client.config.icap_max_header_size,
        )
        .await?;

        match rsp.code {
            100 => {
                let mut body_transfer = match ups_body_type {
                    HttpBodyType::ReadUntilEnd => H1BodyToChunkedTransfer::new_read_until_end(
                        ups_body_io,
                        &mut self.icap_connection.writer,
                        self.copy_config,
                    ),
                    HttpBodyType::ContentLength(len) => H1BodyToChunkedTransfer::new_fixed_length(
                        ups_body_io,
                        &mut self.icap_connection.writer,
                        len,
                        self.copy_config,
                    ),
                    HttpBodyType::Chunked => H1BodyToChunkedTransfer::new_chunked_after_preview(
                        ups_body_io,
                        &mut self.icap_connection.writer,
                        left_chunk_size,
                        self.http_body_line_max_size,
                        self.copy_config,
                    ),
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

                match rsp.code {
                    204 | 206 => {
                        return Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
                            IcapErrorReason::InvalidResponseAfterContinue,
                            rsp.code,
                            rsp.reason,
                        ));
                    }
                    n if (200..300).contains(&n) => {}
                    _ => {
                        return Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
                            IcapErrorReason::UnknownResponseAfterContinue,
                            rsp.code,
                            rsp.reason,
                        ));
                    }
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
                            let icap_keepalive = rsp.keep_alive;
                            let mut bidirectional_transfer = BidirectionalRecvHttpResponse {
                                icap_reader: &mut self.icap_connection.reader,
                                copy_config: self.copy_config,
                                http_body_line_max_size: self.http_body_line_max_size,
                                http_trailer_max_size: self.http_trailer_max_size,
                                idle_checker: &self.idle_checker,
                                http_header_size: header_size,
                                icap_read_finished: false,
                            };
                            let r = bidirectional_transfer
                                .transfer(
                                    state,
                                    &mut body_transfer,
                                    http_response,
                                    clt_send_response,
                                )
                                .await?;
                            let icap_read_finished = bidirectional_transfer.icap_read_finished;
                            if body_transfer.finished() {
                                state.mark_ups_recv_all();
                                self.icap_connection.mark_writer_finished();
                                if icap_read_finished {
                                    self.icap_connection.mark_reader_finished();
                                    if icap_keepalive {
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
                    ups_body_type,
                    ups_body_io,
                    preview_buf,
                    left_chunk_size,
                    clt_send_response,
                )
                .await
            }
            206 => Err(H1ToH2RespmodAdaptationError::NotImplemented(
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
                            http_response,
                            clt_send_response,
                        )
                        .await
                    }
                    IcapRespmodResponsePayload::HttpResponseWithBody(header_size) => {
                        self.handle_icap_http_response_with_body_after_transfer(
                            state,
                            rsp,
                            header_size,
                            http_response,
                            clt_send_response,
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
                Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
                    IcapErrorReason::UnknownResponseForPreview,
                    rsp.code,
                    rsp.reason,
                ))
            }
        }
    }

    async fn read_plain_preview_data<R>(
        &mut self,
        reader: &mut R,
        max_size: usize,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, H1ToH2RespmodAdaptationError>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = vec![0u8; max_size];
        let mut read_offset;
        match tokio::time::timeout(timeout, reader.read(&mut buf)).await {
            Ok(Ok(n)) => read_offset = n,
            Ok(Err(e)) => return Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e)),
            Err(_) => return Ok(None),
        }

        let mut pin_reader = Pin::new(reader);
        while read_offset < max_size {
            let mut read_buf = ReadBuf::new(&mut buf[read_offset..]);
            match poll_fn(
                |cx| match pin_reader.as_mut().poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(_)) => Poll::Ready(Ok(Some(read_buf.filled().len()))),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Pending => Poll::Ready(Ok(None)),
                },
            )
            .await
            {
                Ok(Some(0)) => break,
                Ok(Some(n)) => read_offset += n,
                Ok(None) => break,
                Err(e) => return Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e)),
            }
        }

        buf.truncate(read_offset);
        Ok(Some(buf))
    }

    async fn read_chunked_preview_data<R>(
        &mut self,
        reader: &mut ChunkedDataDecodeReader<'_, R>,
        max_size: usize,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>, H1ToH2RespmodAdaptationError>
    where
        R: AsyncBufRead + Unpin,
    {
        let mut buf = vec![0u8; max_size];
        let mut read_offset;
        match tokio::time::timeout(timeout, reader.read(&mut buf)).await {
            Ok(Ok(n)) => read_offset = n,
            Ok(Err(e)) => return Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e)),
            Err(_) => return Ok(None),
        }

        let mut pin_reader = Pin::new(reader);
        let mut idle_interval = self.idle_checker.interval_timer();
        let mut idle_count = 0;
        let mut is_active = false;

        while read_offset < max_size {
            let mut read_buf = ReadBuf::new(&mut buf[read_offset..]);
            let pin_read = poll_fn(
                |cx| match pin_reader.as_mut().poll_read(cx, &mut read_buf) {
                    Poll::Ready(Ok(_)) => Poll::Ready(Ok(Some(read_buf.filled().len()))),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Pending => {
                        if pin_reader.pending_cancel_safe() {
                            Poll::Ready(Ok(None))
                        } else {
                            Poll::Pending
                        }
                    }
                },
            );

            tokio::select! {
                biased;
                r = pin_read => {
                    match r {
                        Ok(Some(0)) => break,
                        Ok(Some(n)) => {
                            is_active = true;
                            read_offset += n;
                        }
                        Ok(None) => break,
                        Err(e) => return Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e)),
                    }
                }
                n = idle_interval.tick() => {
                    if !is_active {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle);
                        }
                    } else {
                        idle_count = 0;
                        is_active = false;
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }

        buf.truncate(read_offset);
        Ok(Some(buf))
    }

    async fn send_preview_data<H>(
        &mut self,
        http_request: &Request<()>,
        http_response: &H,
        data: &[u8],
    ) -> Result<(), H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
    {
        let http_req_header = http_request.serialize_for_adapter();
        let http_rsp_header = http_response.serialize_for_adapter();
        let icap_header =
            self.build_preview_request(http_req_header.len(), http_rsp_header.len(), data.len());
        let chunk_start = format!("{:x}\r\n", data.len());

        let icap_w = &mut self.icap_connection.writer;
        icap_w
            .write_all_vectored([
                IoSlice::new(&icap_header),
                IoSlice::new(&http_req_header),
                IoSlice::new(&http_rsp_header),
                IoSlice::new(chunk_start.as_bytes()),
                IoSlice::new(data),
                IoSlice::new(b"\r\n0\r\n\r\n"),
            ])
            .await
            .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)?;
        icap_w
            .flush()
            .await
            .map_err(H1ToH2RespmodAdaptationError::IcapServerWriteFailed)
    }
}
