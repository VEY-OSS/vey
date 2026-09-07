/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use h2::client::SendRequest;
use tokio::io::{AsyncBufRead, AsyncWrite};

use vey_http::H1BodyToChunkedTransfer;
use vey_io_ext::{IdleCheck, LimitedBufReadExt, StreamCopyConfig, StreamCopyError};

use super::{H1ToH2ReqmodAdaptationError, H1ToH2ReqmodEndState, H1ToH2ReqmodRunState};
use crate::reqmod::h1::HttpRequestForAdaptation;
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
        mut body_transfer: &mut H1BodyToChunkedTransfer<'_, CR, IcapClientWriter>,
    ) -> Result<ReqmodResponse, H1ToH2ReqmodAdaptationError>
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
                        Ok(_) => self.recv_icap_response().await,
                        Err(StreamCopyError::ReadFailed(e)) => {
                            Err(H1ToH2ReqmodAdaptationError::HttpClientReadFailed(e))
                        }
                        Err(StreamCopyError::WriteFailed(e)) => {
                            Err(H1ToH2ReqmodAdaptationError::IcapServerWriteFailed(e))
                        }
                    };
                }
                r = self.icap_reader.fill_wait_data() => {
                    return match r {
                        Ok(true) => self.recv_icap_response().await,
                        Ok(false) => Err(H1ToH2ReqmodAdaptationError::IcapServerConnectionClosed),
                        Err(e) => Err(H1ToH2ReqmodAdaptationError::IcapServerReadFailed(e)),
                    };
                }
                n = idle_interval.tick() => {
                    if body_transfer.is_idle() {
                        idle_count += n;
                        if self.idle_checker.check_quit(idle_count) {
                            return if body_transfer.no_cached_data() {
                                Err(H1ToH2ReqmodAdaptationError::HttpClientReadIdle)
                            } else {
                                Err(H1ToH2ReqmodAdaptationError::IcapServerWriteIdle)
                            };
                        }
                    } else {
                        idle_count = 0;
                        body_transfer.reset_active();
                    }
                    if let Some(reason) = self.idle_checker.check_force_quit() {
                        return Err(H1ToH2ReqmodAdaptationError::IdleForceQuit(reason));
                    }
                }
            }
        }
    }

    async fn recv_icap_response(self) -> Result<ReqmodResponse, H1ToH2ReqmodAdaptationError> {
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
    pub(super) http_rsp_head_recv_timeout: Duration,
    pub(super) copy_config: StreamCopyConfig,
    pub(super) idle_checker: &'a I,
    pub(super) http_header_size: usize,
    pub(super) icap_read_finished: bool,
}

impl<I: IdleCheck> BidirectionalRecvHttpRequest<'_, I> {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn transfer<H, CR, IW>(
        &mut self,
        _state: &mut H1ToH2ReqmodRunState,
        _ups_body_transfer: &mut H1BodyToChunkedTransfer<'_, CR, IcapClientWriter>,
        _orig_http_request: &H,
        _icap_reader: &mut IcapClientReader,
        _ups_send_req: SendRequest<Bytes>,
        _clt_informational: &mut IW,
        _allow_continue: bool,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
        CR: AsyncBufRead + Unpin,
        IW: AsyncWrite + Unpin,
    {
        let _ = (
            self.http_body_line_max_size,
            self.http_trailer_max_size,
            self.http_req_add_no_via_header,
            self.http_rsp_head_recv_timeout,
            self.copy_config,
            self.http_header_size,
        );
        Err(H1ToH2ReqmodAdaptationError::NotImplemented(
            "h1_to_h2 bidirectional adapted request while body is still uploading",
        ))
    }
}
