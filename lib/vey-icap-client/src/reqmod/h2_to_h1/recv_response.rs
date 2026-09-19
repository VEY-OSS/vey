/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use vey_io_ext::IdleCheck;

use super::{
    H2ToH1ReqmodAdaptationError, H2ToH1RequestAdapter, HttpAdapterErrorResponse,
    HttpRequestForAdaptation, ReqmodAdaptationEndState, ReqmodRecvHttpResponseBody,
};
use crate::reason::IcapErrorReason;
use crate::reqmod::response::ReqmodResponse;

impl<I: IdleCheck> H2ToH1RequestAdapter<I> {
    pub(super) async fn handle_icap_ok_without_payload<H>(
        self,
        icap_rsp: ReqmodResponse,
    ) -> Result<ReqmodAdaptationEndState<H>, H2ToH1ReqmodAdaptationError>
    where
        H: HttpRequestForAdaptation,
    {
        if icap_rsp.keep_alive {
            self.icap_client.save_connection(self.icap_connection);
        }
        Err(H2ToH1ReqmodAdaptationError::IcapServerErrorResponse(
            IcapErrorReason::NoBodyFound,
            icap_rsp.code,
            icap_rsp.reason,
        ))
    }

    pub(super) async fn handle_icap_http_response_with_body(
        mut self,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
    ) -> Result<(HttpAdapterErrorResponse, ReqmodRecvHttpResponseBody), H2ToH1ReqmodAdaptationError>
    {
        let http_rsp =
            HttpAdapterErrorResponse::parse(&mut self.icap_connection.reader, http_header_size)
                .await?;
        let recv_body = ReqmodRecvHttpResponseBody::from_parts(
            self.icap_client,
            icap_rsp.keep_alive,
            self.icap_connection,
            self.copy_config,
            self.http_body_line_max_size,
            self.http_trailer_max_size,
        );
        Ok((http_rsp, recv_body))
    }

    pub(super) async fn handle_icap_http_response_without_body(
        mut self,
        icap_rsp: ReqmodResponse,
        http_header_size: usize,
    ) -> Result<HttpAdapterErrorResponse, H2ToH1ReqmodAdaptationError> {
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
