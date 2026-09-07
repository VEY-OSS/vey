/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use bytes::Bytes;
use h2::SendStream;
use h2::client::SendRequest;
use http::Request;
use tokio::io::{AsyncBufRead, AsyncRead, AsyncReadExt, AsyncWrite};

use vey_h2::{
    H2BodyEncodeTransfer, H2ResponseHeaderReceiver, H2StreamBodyEncodeTransferError,
    H2StreamFromChunkedTransfer, H2StreamFromChunkedTransferError,
};
use vey_http::server::HttpAdaptedRequest;
use vey_http::{HttpBodyDecodeReader, HttpBodyType};
use vey_io_ext::{IdleCheck, StreamCopyConfig};

use super::super::adapt_request_to_h2;
use super::{
    H1ToH2ReqmodAdaptationError, H1ToH2ReqmodEndState, H1ToH2ReqmodRunState, H1ToH2RequestAdapter,
    orig_h2_request, recv_ups_response_head_after_transfer, take_final_response,
};
use crate::reqmod::h1::HttpRequestForAdaptation;
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H1ToH2RequestAdapter<I> {
    pub(super) async fn send_original_without_body<H, IW>(
        self,
        state: &mut H1ToH2ReqmodRunState,
        icap_rsp: ReqmodResponse,
        http_request: &H,
        mut ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        IW: AsyncWrite + Unpin,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let h2_req = orig_h2_request(http_request);
        let (rsp_fut, _) = ups_send_req
            .send_request(h2_req, true)
            .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();
        state.mark_ups_send_no_body();

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);
        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_informational,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();
        Ok(H1ToH2ReqmodEndState::OriginalTransferred(ups_rsp))
    }

    pub(super) async fn send_adapted_without_body<H, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: &H,
        mut ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        IW: AsyncWrite + Unpin,
    {
        let http_req = HttpAdaptedRequest::parse(
            &mut self.icap_connection.reader,
            http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let final_req = adapt_request_to_h2(orig_h2_request(orig_http_request), &http_req);
        let (rsp_fut, _) = ups_send_req
            .send_request(final_req, true)
            .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();
        state.mark_ups_send_no_body();

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);
        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_informational,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();
        Ok(H1ToH2ReqmodEndState::AdaptedTransferred(http_req, ups_rsp))
    }

    pub(super) async fn send_adapted_with_body<H, IW>(
        mut self,
        state: &mut H1ToH2ReqmodRunState,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
        orig_http_request: &H,
        mut ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        IW: AsyncWrite + Unpin,
    {
        let http_req = HttpAdaptedRequest::parse(
            &mut self.icap_connection.reader,
            http_header_size,
            self.http_req_add_no_via_header,
        )
        .await?;

        let final_req = adapt_request_to_h2(orig_h2_request(orig_http_request), &http_req);
        let (rsp_fut, mut ups_send_stream) = ups_send_req
            .send_request(final_req, false)
            .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();

        let idle_checker = &self.idle_checker;
        let allow_continue = self.allow_continue;
        let mut body_transfer = H2StreamFromChunkedTransfer::new(
            &mut self.icap_connection.reader,
            &mut ups_send_stream,
            &self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );
        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);
        Self::pump_h2_body_and_recv_header(
            idle_checker,
            allow_continue,
            state,
            &mut body_transfer,
            &mut ups_recv_rsp,
            clt_informational,
        )
        .await?;

        if body_transfer.finished() {
            self.icap_connection.mark_reader_finished();
            if icap_rsp.keep_alive {
                self.icap_client.save_connection(self.icap_connection);
            }
        }

        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_informational,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();
        Ok(H1ToH2ReqmodEndState::AdaptedTransferred(http_req, ups_rsp))
    }

    async fn pump_h2_body_and_recv_header<IW>(
        idle_checker: &I,
        mut allow_continue: bool,
        state: &mut H1ToH2ReqmodRunState,
        body_transfer: &mut H2StreamFromChunkedTransfer<'_, impl AsyncBufRead + Unpin>,
        ups_recv_rsp: &mut H2ResponseHeaderReceiver,
        clt_informational: &mut IW,
    ) -> Result<(), H1ToH2ReqmodAdaptationError>
    where
        IW: AsyncWrite + Unpin,
    {
        let mut idle_interval = idle_checker.interval_timer();
        let mut idle_count = 0;

        loop {
            tokio::select! {
                biased;

                r = ups_recv_rsp.recv_header() => {
                    match r {
                        Ok(ups_rsp) => {
                            if take_final_response(ups_rsp, clt_informational, &mut allow_continue)
                                .await?
                                .is_some()
                            {
                                state.mark_ups_recv_header();
                                // Final response arrived while body is still in flight; keep
                                // pumping body from the caller via recv_ups_response_head.
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            return Err(H1ToH2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed(e));
                        }
                    }
                }
                r = &mut *body_transfer => {
                    return match r {
                        Ok(_) => {
                            state.mark_ups_send_all();
                            Ok(())
                        }
                        Err(H2StreamFromChunkedTransferError::ReadError(e)) => {
                            Err(H1ToH2ReqmodAdaptationError::IcapServerReadFailed(e))
                        }
                        Err(H2StreamFromChunkedTransferError::SendDataFailed(e)) => {
                            Err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e))
                        }
                        Err(H2StreamFromChunkedTransferError::SendTrailerFailed(e)) => {
                            Err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendTrailerFailed(e))
                        }
                        Err(H2StreamFromChunkedTransferError::SenderNotInSendState) => {
                            Err(H1ToH2ReqmodAdaptationError::HttpUpstreamNotInSendState)
                        }
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if idle_checker.check_quit(idle_count) {
                            return if body_transfer.no_cached_data() {
                                Err(H1ToH2ReqmodAdaptationError::IcapServerReadIdle)
                            } else {
                                Err(H1ToH2ReqmodAdaptationError::HttpUpstreamWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_transfer.reset_active();
                    }
                    if let Some(reason) = idle_checker.check_force_quit() {
                        return Err(H1ToH2ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    pub(super) async fn send_original_h1_body_to_h2<CR, IW>(
        self,
        state: &mut H1ToH2ReqmodRunState,
        icap_rsp: ReqmodResponse,
        h2_req: Request<()>,
        body_type: HttpBodyType,
        clt_body_io: &mut CR,
        preview_buf: Vec<u8>,
        left_chunk_size: u64,
        mut ups_send_req: SendRequest<Bytes>,
        clt_informational: &mut IW,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        CR: AsyncBufRead + Unpin,
        IW: AsyncWrite + Unpin,
    {
        let keep_alive = icap_rsp.keep_alive;

        let (rsp_fut, mut ups_send_stream) = ups_send_req
            .send_request(h2_req, false)
            .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendHeadFailed)?;
        state.mark_ups_send_header();

        if !preview_buf.is_empty() {
            ups_send_stream
                .send_data(Bytes::from(preview_buf), false)
                .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendDataFailed)?;
        }

        match body_type {
            HttpBodyType::Chunked => {
                self.forward_remaining_h1_chunked_to_h2(
                    clt_body_io,
                    left_chunk_size,
                    &mut ups_send_stream,
                )
                .await?;
            }
            HttpBodyType::ContentLength(_) | HttpBodyType::ReadUntilEnd => {
                self.forward_h1_plain_to_h2(clt_body_io, body_type, &mut ups_send_stream)
                    .await?;
            }
        }
        state.mark_ups_send_all();
        state.clt_read_finished = true;

        let mut ups_recv_rsp = H2ResponseHeaderReceiver::new(rsp_fut);
        let ups_rsp = recv_ups_response_head_after_transfer(
            &mut ups_recv_rsp,
            clt_informational,
            self.allow_continue,
            self.http_rsp_head_recv_timeout,
        )
        .await?;
        state.mark_ups_recv_header();
        if keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Ok(H1ToH2ReqmodEndState::OriginalTransferred(ups_rsp))
    }

    async fn forward_h1_plain_to_h2<CR>(
        &self,
        clt_body_io: &mut CR,
        body_type: HttpBodyType,
        send_stream: &mut SendStream<Bytes>,
    ) -> Result<(), H1ToH2ReqmodAdaptationError>
    where
        CR: AsyncBufRead + Unpin,
    {
        let mut body_reader =
            HttpBodyDecodeReader::new(clt_body_io, body_type, self.http_body_line_max_size);
        encode_reader_to_h2(
            &self.idle_checker,
            &self.copy_config,
            &mut body_reader,
            send_stream,
        )
        .await?;
        send_stream
            .send_data(Bytes::new(), true)
            .map_err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendDataFailed)
    }

    async fn forward_remaining_h1_chunked_to_h2<CR>(
        &self,
        clt_body_io: &mut CR,
        left_chunk_size: u64,
        send_stream: &mut SendStream<Bytes>,
    ) -> Result<(), H1ToH2ReqmodAdaptationError>
    where
        CR: AsyncBufRead + Unpin,
    {
        if left_chunk_size > 0 {
            let mut rest = HttpBodyDecodeReader::new_fixed_length(clt_body_io, left_chunk_size);
            encode_reader_to_h2(
                &self.idle_checker,
                &self.copy_config,
                &mut rest,
                send_stream,
            )
            .await?;
        }
        consume_crlf(clt_body_io)
            .await
            .map_err(H1ToH2ReqmodAdaptationError::HttpClientReadFailed)?;

        let mut body_transfer = H2StreamFromChunkedTransfer::new(
            clt_body_io,
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
) -> Result<(), H1ToH2ReqmodAdaptationError>
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
                        Err(H1ToH2ReqmodAdaptationError::HttpClientReadFailed(e))
                    }
                    Err(H2StreamBodyEncodeTransferError::SendDataFailed(e)) => {
                        Err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e))
                    }
                    Err(H2StreamBodyEncodeTransferError::SenderNotInSendState) => {
                        Err(H1ToH2ReqmodAdaptationError::HttpUpstreamNotInSendState)
                    }
                };
            }
            n = idle_interval.tick() => {
                if encode.is_idle() {
                    idle_count += n;
                    if idle_checker.check_quit(idle_count) {
                        return if encode.no_cached_data() {
                            Err(H1ToH2ReqmodAdaptationError::HttpClientReadIdle)
                        } else {
                            Err(H1ToH2ReqmodAdaptationError::HttpUpstreamWriteIdle)
                        };
                    }
                } else {
                    idle_count = 0;
                    encode.reset_active();
                }
                if let Some(reason) = idle_checker.check_force_quit() {
                    return Err(H1ToH2ReqmodAdaptationError::IdleForceQuit(reason));
                }
            }
        }
    }
}

async fn pump_from_chunked<I, R>(
    idle_checker: &I,
    body_transfer: &mut H2StreamFromChunkedTransfer<'_, R>,
) -> Result<(), H1ToH2ReqmodAdaptationError>
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
                    Err(H2StreamFromChunkedTransferError::ReadError(e)) => {
                        Err(H1ToH2ReqmodAdaptationError::HttpClientReadFailed(e))
                    }
                    Err(H2StreamFromChunkedTransferError::SendDataFailed(e)) => {
                        Err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e))
                    }
                    Err(H2StreamFromChunkedTransferError::SendTrailerFailed(e)) => {
                        Err(H1ToH2ReqmodAdaptationError::HttpUpstreamSendTrailerFailed(e))
                    }
                    Err(H2StreamFromChunkedTransferError::SenderNotInSendState) => {
                        Err(H1ToH2ReqmodAdaptationError::HttpUpstreamNotInSendState)
                    }
                };
            }
            n = idle_interval.tick() => {
                if body_transfer.is_idle() {
                    idle_count += n;
                    if idle_checker.check_quit(idle_count) {
                        return if body_transfer.no_cached_data() {
                            Err(H1ToH2ReqmodAdaptationError::HttpClientReadIdle)
                        } else {
                            Err(H1ToH2ReqmodAdaptationError::HttpUpstreamWriteIdle)
                        };
                    }
                } else {
                    idle_count = 0;
                    body_transfer.reset_active();
                }
                if let Some(reason) = idle_checker.check_force_quit() {
                    return Err(H1ToH2ReqmodAdaptationError::IdleForceQuit(reason));
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
) -> H1ToH2ReqmodAdaptationError {
    match e {
        H2StreamFromChunkedTransferError::ReadError(e) => {
            H1ToH2ReqmodAdaptationError::IcapServerReadFailed(e)
        }
        H2StreamFromChunkedTransferError::SendDataFailed(e) => {
            H1ToH2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e)
        }
        H2StreamFromChunkedTransferError::SendTrailerFailed(e) => {
            H1ToH2ReqmodAdaptationError::HttpUpstreamSendTrailerFailed(e)
        }
        H2StreamFromChunkedTransferError::SenderNotInSendState => {
            H1ToH2ReqmodAdaptationError::HttpUpstreamNotInSendState
        }
    }
}
