/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_io_ext::IdleCheck;

use super::{H1ToH2ReqmodAdaptationError, H1ToH2ReqmodEndState, H1ToH2RequestAdapter};
use crate::reason::IcapErrorReason;
use crate::reqmod::h1::{HttpAdapterErrorResponse, ReqmodRecvHttpResponseBody};
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H1ToH2RequestAdapter<I> {
    pub(super) async fn handle_icap_ok_without_payload(
        self,
        icap_rsp: ReqmodResponse,
    ) -> Result<H1ToH2ReqmodEndState, H1ToH2ReqmodAdaptationError> {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Err(H1ToH2ReqmodAdaptationError::IcapServerErrorResponse(
            IcapErrorReason::NoBodyFound,
            icap_rsp.code,
            icap_rsp.reason,
        ))
    }

    pub(super) async fn handle_icap_http_response_with_body(
        mut self,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
    ) -> Result<(HttpAdapterErrorResponse, ReqmodRecvHttpResponseBody), H1ToH2ReqmodAdaptationError>
    {
        let mut http_rsp =
            HttpAdapterErrorResponse::parse(&mut self.icap_connection.reader, http_header_size)
                .await?;
        http_rsp.set_chunked_encoding();
        let recv_body = ReqmodRecvHttpResponseBody::from_connection(
            self.icap_client,
            icap_rsp.keep_alive,
            self.icap_connection,
        );
        Ok((http_rsp, recv_body))
    }

    pub(super) async fn handle_icap_http_response_without_body(
        mut self,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
    ) -> Result<HttpAdapterErrorResponse, H1ToH2ReqmodAdaptationError> {
        let http_rsp =
            HttpAdapterErrorResponse::parse(&mut self.icap_connection.reader, http_header_size)
                .await?;
        self.icap_connection.mark_reader_finished();
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Ok(http_rsp)
    }
}
