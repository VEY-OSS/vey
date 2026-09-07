/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use bytes::Bytes;
use h2::SendStream;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt};

use vey_h2::{
    H2BodyEncodeTransfer, H2StreamBodyEncodeTransferError, H2StreamFromChunkedTransfer,
    H2StreamFromChunkedTransferError,
};
use vey_http::client::HttpAdaptedResponse;
use vey_http::{HttpBodyDecodeReader, HttpBodyType};
use vey_io_ext::{IdleCheck, StreamCopyConfig};

use super::super::adapt_response_to_h2;
use super::{
    H1ToH2RespmodAdaptationError, H1ToH2RespmodEndState, H1ToH2RespmodRunState,
    H1ToH2ResponseAdapter, orig_h2_response,
};
use crate::reason::IcapErrorReason;
use crate::respmod::h1::HttpResponseForAdaptation;
use crate::respmod::h2::H2SendResponseToClient;
use crate::respmod::response::RespmodResponse;

impl<I: IdleCheck> H1ToH2ResponseAdapter<I> {
    pub(super) async fn handle_icap_ok_without_payload(
        self,
        icap_rsp: RespmodResponse,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError> {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Err(H1ToH2RespmodAdaptationError::IcapServerErrorResponse(
            IcapErrorReason::NoBodyFound,
            icap_rsp.code,
            icap_rsp.reason,
        ))
    }

    pub(super) async fn handle_original_http_response_without_body<H, CW>(
        self,
        state: &mut H1ToH2RespmodRunState,
        icap_rsp: RespmodResponse,
        http_response: &H,
        clt_send_response: &mut CW,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        CW: H2SendResponseToClient,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        state.mark_clt_send_start();
        clt_send_response
            .send_response(orig_h2_response(http_response), true)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();
        state.mark_clt_send_no_body();

        Ok(H1ToH2RespmodEndState::OriginalTransferred)
    }

    pub(super) async fn handle_original_http_response_with_body<H, UR, CW>(
        self,
        state: &mut H1ToH2RespmodRunState,
        icap_rsp: RespmodResponse,
        http_response: &H,
        body_type: HttpBodyType,
        ups_body_io: &mut UR,
        preview_buf: Vec<u8>,
        left_chunk_size: u64,
        clt_send_response: &mut CW,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        UR: AsyncBufRead + Unpin,
        CW: H2SendResponseToClient,
    {
        let keep_alive = icap_rsp.keep_alive;

        state.mark_clt_send_start();
        let mut clt_send_stream = clt_send_response
            .send_response(orig_h2_response(http_response), false)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();

        if !preview_buf.is_empty() {
            clt_send_stream
                .send_data(Bytes::from(preview_buf), false)
                .map_err(H1ToH2RespmodAdaptationError::HttpClientSendDataFailed)?;
        }

        match body_type {
            HttpBodyType::Chunked => {
                self.forward_remaining_h1_chunked_to_h2(
                    ups_body_io,
                    left_chunk_size,
                    &mut clt_send_stream,
                )
                .await?;
            }
            HttpBodyType::ContentLength(_) | HttpBodyType::ReadUntilEnd => {
                self.forward_h1_plain_to_h2(ups_body_io, body_type, &mut clt_send_stream)
                    .await?;
            }
        }
        state.mark_ups_recv_all();
        state.mark_clt_send_all();
        if keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Ok(H1ToH2RespmodEndState::OriginalTransferred)
    }

    pub(super) async fn handle_icap_http_response_without_body<H, CW>(
        mut self,
        state: &mut H1ToH2RespmodRunState,
        icap_rsp: RespmodResponse,
        http_header_size: usize,
        orig_http_response: &H,
        clt_send_response: &mut CW,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        CW: H2SendResponseToClient,
    {
        let http_rsp =
            HttpAdaptedResponse::parse(&mut self.icap_connection.reader, http_header_size).await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let final_rsp = adapt_response_to_h2(orig_h2_response(orig_http_response), &http_rsp);
        state.mark_clt_send_start();
        clt_send_response
            .send_response(final_rsp, true)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();
        state.mark_clt_send_no_body();

        Ok(H1ToH2RespmodEndState::AdaptedTransferred(http_rsp))
    }

    pub(super) async fn handle_icap_http_response_with_body_after_transfer<H, CW>(
        mut self,
        state: &mut H1ToH2RespmodRunState,
        icap_rsp: RespmodResponse,
        http_header_size: usize,
        orig_http_response: &H,
        clt_send_response: &mut CW,
    ) -> Result<H1ToH2RespmodEndState, H1ToH2RespmodAdaptationError>
    where
        H: HttpResponseForAdaptation,
        CW: H2SendResponseToClient,
    {
        let http_rsp =
            HttpAdaptedResponse::parse(&mut self.icap_connection.reader, http_header_size).await?;

        let final_rsp = adapt_response_to_h2(orig_h2_response(orig_http_response), &http_rsp);
        state.mark_clt_send_start();
        let mut clt_send_stream = clt_send_response
            .send_response(final_rsp, false)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed)?;
        state.mark_clt_send_header();

        let mut body_transfer = H2StreamFromChunkedTransfer::new(
            &mut self.icap_connection.reader,
            &mut clt_send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );
        pump_from_chunked(&self.idle_checker, &mut body_transfer).await?;

        state.mark_clt_send_all();
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Ok(H1ToH2RespmodEndState::AdaptedTransferred(http_rsp))
    }

    async fn forward_h1_plain_to_h2<UR>(
        &self,
        ups_body_io: &mut UR,
        body_type: HttpBodyType,
        send_stream: &mut SendStream<Bytes>,
    ) -> Result<(), H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
    {
        let mut body_reader =
            HttpBodyDecodeReader::new(ups_body_io, body_type, self.http_body_line_max_size);
        encode_reader_to_h2(
            &self.idle_checker,
            &self.copy_config,
            &mut body_reader,
            send_stream,
        )
        .await?;
        send_stream
            .send_data(Bytes::new(), true)
            .map_err(H1ToH2RespmodAdaptationError::HttpClientSendDataFailed)
    }

    async fn forward_remaining_h1_chunked_to_h2<UR>(
        &self,
        ups_body_io: &mut UR,
        left_chunk_size: u64,
        send_stream: &mut SendStream<Bytes>,
    ) -> Result<(), H1ToH2RespmodAdaptationError>
    where
        UR: AsyncBufRead + Unpin,
    {
        if left_chunk_size > 0 {
            let mut rest = HttpBodyDecodeReader::new_fixed_length(ups_body_io, left_chunk_size);
            encode_reader_to_h2(
                &self.idle_checker,
                &self.copy_config,
                &mut rest,
                send_stream,
            )
            .await?;
        }
        consume_crlf(ups_body_io)
            .await
            .map_err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed)?;

        let mut body_transfer = H2StreamFromChunkedTransfer::new(
            ups_body_io,
            send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );
        pump_from_chunked(&self.idle_checker, &mut body_transfer).await
    }
}

async fn encode_reader_to_h2<I, R>(
    idle_checker: &I,
    copy_config: &StreamCopyConfig,
    reader: &mut R,
    send_stream: &mut SendStream<Bytes>,
) -> Result<(), H1ToH2RespmodAdaptationError>
where
    I: IdleCheck,
    R: AsyncRead + Unpin,
{
    let mut encode = H2BodyEncodeTransfer::new(reader, send_stream, copy_config);
    let mut idle_interval = idle_checker.interval_timer();
    let mut idle_count = 0;
    loop {
        tokio::select! {
            biased;
            r = &mut encode => {
                return match r {
                    Ok(()) => Ok(()),
                    Err(H2StreamBodyEncodeTransferError::ReadError(e)) => {
                        Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e))
                    }
                    Err(H2StreamBodyEncodeTransferError::SendDataFailed(e)) => {
                        Err(H1ToH2RespmodAdaptationError::HttpClientSendDataFailed(e))
                    }
                    Err(H2StreamBodyEncodeTransferError::SenderNotInSendState) => {
                        Err(H1ToH2RespmodAdaptationError::HttpClientNotInSendState)
                    }
                };
            }
            n = idle_interval.tick() => {
                if encode.is_idle() {
                    idle_count += n;
                    if idle_checker.check_quit(idle_count) {
                        return if encode.no_cached_data() {
                            Err(H1ToH2RespmodAdaptationError::HttpUpstreamReadIdle)
                        } else {
                            Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                        };
                    }
                } else {
                    idle_count = 0;
                    encode.reset_active();
                }
                if let Some(reason) = idle_checker.check_force_quit() {
                    return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                }
            }
        }
    }
}

async fn pump_from_chunked<I, R>(
    idle_checker: &I,
    body_transfer: &mut H2StreamFromChunkedTransfer<'_, R>,
) -> Result<(), H1ToH2RespmodAdaptationError>
where
    I: IdleCheck,
    R: AsyncBufRead + Unpin,
{
    let mut idle_interval = idle_checker.interval_timer();
    let mut idle_count = 0;
    loop {
        tokio::select! {
            biased;
            r = &mut *body_transfer => {
                return match r {
                    Ok(()) => Ok(()),
                    Err(e) => Err(map_from_chunked_err(e)),
                };
            }
            n = idle_interval.tick() => {
                if body_transfer.is_idle() {
                    idle_count += n;
                    if idle_checker.check_quit(idle_count) {
                        return if body_transfer.no_cached_data() {
                            Err(H1ToH2RespmodAdaptationError::IcapServerReadIdle)
                        } else {
                            Err(H1ToH2RespmodAdaptationError::HttpClientWriteIdle)
                        };
                    }
                } else {
                    idle_count = 0;
                    body_transfer.reset_active();
                }
                if let Some(reason) = idle_checker.check_force_quit() {
                    return Err(H1ToH2RespmodAdaptationError::IdleForceQuit(reason));
                }
            }
        }
    }
}

async fn consume_crlf<R>(reader: &mut R) -> io::Result<()>
where
    R: AsyncBufRead + Unpin,
{
    let mut buf = [0u8; 2];
    reader.read_exact(&mut buf).await?;
    if buf == *b"\r\n" {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "expected CRLF after chunk data",
        ))
    }
}

pub(super) fn map_from_chunked_err(
    e: H2StreamFromChunkedTransferError,
) -> H1ToH2RespmodAdaptationError {
    match e {
        H2StreamFromChunkedTransferError::ReadError(e) => {
            H1ToH2RespmodAdaptationError::IcapServerReadFailed(e)
        }
        H2StreamFromChunkedTransferError::SendDataFailed(e) => {
            H1ToH2RespmodAdaptationError::HttpClientSendDataFailed(e)
        }
        H2StreamFromChunkedTransferError::SendTrailerFailed(e) => {
            H1ToH2RespmodAdaptationError::HttpClientSendTrailerFailed(e)
        }
        H2StreamFromChunkedTransferError::SenderNotInSendState => {
            H1ToH2RespmodAdaptationError::HttpClientNotInSendState
        }
    }
}
