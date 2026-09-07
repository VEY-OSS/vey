/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::IoSlice;

use h2::RecvStream;
use http::Response;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use vey_h2::{H2PreviewData, H2StreamToChunkedTransfer, H2StreamToChunkedTransferError};
use vey_http::HttpBodyReader;
use vey_http::client::HttpAdaptedResponse;
use vey_io_ext::{IdleCheck, LimitedWriteExt, StreamCopy, StreamCopyError};

use super::super::{serialize_adapted_response_as_h1_chunked, serialize_h2_response_as_h1_chunked};
use super::{
    H2ToH1RespmodAdaptationError, H2ToH1RespmodEndState, H2ToH1RespmodRunState,
    H2ToH1ResponseAdapter,
};
use crate::reason::IcapErrorReason;
use crate::respmod::response::RespmodResponse;

impl<I: IdleCheck> H2ToH1ResponseAdapter<I> {
    pub(super) async fn handle_icap_ok_without_payload(
        self,
        icap_rsp: RespmodResponse,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError> {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Err(H2ToH1RespmodAdaptationError::IcapServerErrorResponse(
            IcapErrorReason::NoBodyFound,
            icap_rsp.code,
            icap_rsp.reason,
        ))
    }

    pub(super) async fn handle_original_http_response_without_body<CW>(
        self,
        state: &mut H2ToH1RespmodRunState,
        icap_rsp: RespmodResponse,
        http_response: Response<()>,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        CW: AsyncWrite + Unpin,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let head = serialize_h2_response_as_h1_chunked(&http_response, false);
        state.mark_clt_send_start();
        clt_writer
            .write_all(&head)
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        clt_writer
            .flush()
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        state.mark_clt_send_header();
        state.mark_clt_send_no_body();

        Ok(H2ToH1RespmodEndState::OriginalTransferred)
    }

    pub(super) async fn handle_original_http_response_with_body<CW>(
        self,
        state: &mut H2ToH1RespmodRunState,
        icap_rsp: RespmodResponse,
        http_response: Response<()>,
        mut preview_data: H2PreviewData,
        mut ups_body: RecvStream,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        CW: AsyncWrite + Unpin,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let head = serialize_h2_response_as_h1_chunked(&http_response, true);
        state.mark_clt_send_start();
        clt_writer
            .write_all(&head)
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        state.mark_clt_send_header();

        if !preview_data.preview_buf().is_empty() {
            let chunk_header = format!("{:x}\r\n", preview_data.preview_size());
            clt_writer
                .write_all_vectored([
                    IoSlice::new(chunk_header.as_bytes()),
                    IoSlice::new(preview_data.preview_buf()),
                    IoSlice::new(b"\r\n"),
                ])
                .await
                .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        }

        let mut body_transfer = if let Some(left) = preview_data.take_left() {
            H2StreamToChunkedTransfer::with_chunk(
                &mut ups_body,
                clt_writer,
                self.copy_config.yield_size(),
                left,
            )
        } else {
            H2StreamToChunkedTransfer::new(&mut ups_body, clt_writer, self.copy_config.yield_size())
        };
        pump_to_chunked(&self.idle_checker, &mut body_transfer).await?;

        state.mark_ups_recv_all();
        state.mark_clt_send_all();
        Ok(H2ToH1RespmodEndState::OriginalTransferred)
    }

    pub(super) async fn handle_icap_http_response_without_body<CW>(
        mut self,
        state: &mut H2ToH1RespmodRunState,
        icap_rsp: RespmodResponse,
        http_header_size: usize,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        CW: AsyncWrite + Unpin,
    {
        let http_rsp =
            HttpAdaptedResponse::parse(&mut self.icap_connection.reader, http_header_size).await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }

        let head = serialize_adapted_response_as_h1_chunked(&http_rsp, false);
        state.mark_clt_send_start();
        clt_writer
            .write_all(&head)
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        clt_writer
            .flush()
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        state.mark_clt_send_header();
        state.mark_clt_send_no_body();

        Ok(H2ToH1RespmodEndState::AdaptedTransferred(http_rsp))
    }

    pub(super) async fn handle_icap_http_response_with_body_after_transfer<CW>(
        mut self,
        state: &mut H2ToH1RespmodRunState,
        icap_rsp: RespmodResponse,
        http_header_size: usize,
        clt_writer: &mut CW,
    ) -> Result<H2ToH1RespmodEndState, H2ToH1RespmodAdaptationError>
    where
        CW: AsyncWrite + Unpin,
    {
        let http_rsp =
            HttpAdaptedResponse::parse(&mut self.icap_connection.reader, http_header_size).await?;

        let head = serialize_adapted_response_as_h1_chunked(&http_rsp, true);
        state.mark_clt_send_start();
        clt_writer
            .write_all(&head)
            .await
            .map_err(H2ToH1RespmodAdaptationError::HttpClientWriteFailed)?;
        state.mark_clt_send_header();

        let mut body_reader = HttpBodyReader::new_chunked(
            &mut self.icap_connection.reader,
            self.http_body_line_max_size,
        );
        let mut body_copy = StreamCopy::new(&mut body_reader, clt_writer, &self.copy_config);
        send_response_body(&self.idle_checker, &mut body_copy).await?;

        state.mark_clt_send_all();
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Ok(H2ToH1RespmodEndState::AdaptedTransferred(http_rsp))
    }
}

pub(super) async fn pump_to_chunked_icap<I, W>(
    idle_checker: &I,
    body_transfer: &mut H2StreamToChunkedTransfer<'_, W>,
) -> Result<(), H2ToH1RespmodAdaptationError>
where
    I: IdleCheck,
    W: AsyncWrite + Unpin,
{
    pump_to_chunked_with(idle_checker, body_transfer, map_to_chunked_icap_err, true).await
}

pub(super) async fn pump_to_chunked<I, W>(
    idle_checker: &I,
    body_transfer: &mut H2StreamToChunkedTransfer<'_, W>,
) -> Result<(), H2ToH1RespmodAdaptationError>
where
    I: IdleCheck,
    W: AsyncWrite + Unpin,
{
    pump_to_chunked_with(
        idle_checker,
        body_transfer,
        map_to_chunked_client_err,
        false,
    )
    .await
}

fn map_to_chunked_client_err(e: H2StreamToChunkedTransferError) -> H2ToH1RespmodAdaptationError {
    match e {
        H2StreamToChunkedTransferError::WriteError(e) => {
            H2ToH1RespmodAdaptationError::HttpClientWriteFailed(e)
        }
        H2StreamToChunkedTransferError::RecvDataFailed(e) => {
            H2ToH1RespmodAdaptationError::HttpUpstreamRecvDataFailed(e)
        }
        H2StreamToChunkedTransferError::RecvTrailerFailed(e) => {
            H2ToH1RespmodAdaptationError::HttpUpstreamRecvTrailerFailed(e)
        }
    }
}

async fn pump_to_chunked_with<I, W>(
    idle_checker: &I,
    body_transfer: &mut H2StreamToChunkedTransfer<'_, W>,
    map_err: fn(H2StreamToChunkedTransferError) -> H2ToH1RespmodAdaptationError,
    icap_write: bool,
) -> Result<(), H2ToH1RespmodAdaptationError>
where
    I: IdleCheck,
    W: AsyncWrite + Unpin,
{
    let mut idle_interval = idle_checker.interval_timer();
    let mut idle_count = 0;
    loop {
        tokio::select! {
            biased;
            r = &mut *body_transfer => {
                return match r {
                    Ok(_) => Ok(()),
                    Err(e) => Err(map_err(e)),
                };
            }
            n = idle_interval.tick() => {
                if body_transfer.is_idle() {
                    idle_count += n;
                    if idle_checker.check_quit(idle_count) {
                        return if body_transfer.no_cached_data() {
                            Err(H2ToH1RespmodAdaptationError::HttpUpstreamReadIdle)
                        } else if icap_write {
                            Err(H2ToH1RespmodAdaptationError::IcapServerWriteIdle)
                        } else {
                            Err(H2ToH1RespmodAdaptationError::HttpClientWriteIdle)
                        };
                    }
                } else {
                    idle_count = 0;
                    body_transfer.reset_active();
                }
                if let Some(reason) = idle_checker.check_force_quit() {
                    return Err(H2ToH1RespmodAdaptationError::IdleForceQuit(reason));
                }
            }
        }
    }
}

async fn send_response_body<I, R, W>(
    idle_checker: &I,
    mut body_copy: &mut StreamCopy<'_, R, W>,
) -> Result<(), H2ToH1RespmodAdaptationError>
where
    I: IdleCheck,
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut idle_interval = idle_checker.interval_timer();
    let mut idle_count = 0;
    loop {
        tokio::select! {
            biased;
            r = &mut body_copy => {
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
                if body_copy.is_idle() {
                    idle_count += n;
                    if idle_checker.check_quit(idle_count) {
                        return if body_copy.no_cached_data() {
                            Err(H2ToH1RespmodAdaptationError::IcapServerReadIdle)
                        } else {
                            Err(H2ToH1RespmodAdaptationError::HttpClientWriteIdle)
                        };
                    }
                } else {
                    idle_count = 0;
                    body_copy.reset_active();
                }
                if let Some(reason) = idle_checker.check_force_quit() {
                    return Err(H2ToH1RespmodAdaptationError::IdleForceQuit(reason));
                }
            }
        }
    }
}

pub(super) fn map_to_chunked_icap_err(
    e: H2StreamToChunkedTransferError,
) -> H2ToH1RespmodAdaptationError {
    match e {
        H2StreamToChunkedTransferError::WriteError(e) => {
            H2ToH1RespmodAdaptationError::IcapServerWriteFailed(e)
        }
        H2StreamToChunkedTransferError::RecvDataFailed(e) => {
            H2ToH1RespmodAdaptationError::HttpUpstreamRecvDataFailed(e)
        }
        H2StreamToChunkedTransferError::RecvTrailerFailed(e) => {
            H2ToH1RespmodAdaptationError::HttpUpstreamRecvTrailerFailed(e)
        }
    }
}
