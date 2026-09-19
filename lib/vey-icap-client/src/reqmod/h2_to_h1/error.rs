/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

use vey_h2::{H2PreviewError, H2StreamToChunkedTransferError};
use vey_http::client::HttpResponseParseError;
use vey_http::server::HttpRequestParseError;
use vey_io_ext::IdleForceQuitReason;

use crate::reason::IcapErrorReason;
use crate::reqmod::IcapReqmodParseError;

#[derive(Debug, Error)]
pub enum H2ToH1ReqmodAdaptationError {
    #[error("write to icap server failed: {0:?}")]
    IcapServerWriteFailed(io::Error),
    #[error("read from icap server failed: {0:?}")]
    IcapServerReadFailed(io::Error),
    #[error("connection closed by icap server")]
    IcapServerConnectionClosed,
    #[error("invalid response from icap server: {0}")]
    InvalidIcapServerResponse(#[from] IcapReqmodParseError),
    #[error("invalid http error response from icap server: {0}")]
    InvalidIcapServerHttpResponse(#[from] HttpResponseParseError),
    #[error("invalid http request from icap server: {0}")]
    InvalidIcapServerHttpRequest(#[from] HttpRequestParseError),
    #[error("invalid http body from icap server: {0:?}")]
    InvalidHttpBodyFromIcapServer(anyhow::Error),
    #[error("error response from icap server: {0} ({1} {2})")]
    IcapServerErrorResponse(IcapErrorReason, u16, String),
    #[error("recv data from http client failed: {0}")]
    HttpClientRecvDataFailed(h2::Error),
    #[error("recv trailer from http client failed: {0}")]
    HttpClientRecvTrailerFailed(h2::Error),
    #[error("write to http upstream failed: {0:?}")]
    HttpUpstreamWriteFailed(io::Error),
    #[error("force quit from idle checker: {0:?}")]
    IdleForceQuit(IdleForceQuitReason),
    #[error("idle while reading from http client")]
    HttpClientReadIdle,
    #[error("idle while writing to http upstream")]
    HttpUpstreamWriteIdle,
    #[error("idle while reading from icap server")]
    IcapServerReadIdle,
    #[error("idle while writing to icap server")]
    IcapServerWriteIdle,
    #[error("not implemented feature: {0}")]
    NotImplemented(&'static str),
}

impl H2ToH1ReqmodAdaptationError {
    /// The client body is being written to the ICAP server as chunked.
    pub(super) fn clt_to_icap(e: H2StreamToChunkedTransferError) -> Self {
        match e {
            H2StreamToChunkedTransferError::WriteError(e) => {
                H2ToH1ReqmodAdaptationError::IcapServerWriteFailed(e)
            }
            H2StreamToChunkedTransferError::RecvDataFailed(e) => {
                H2ToH1ReqmodAdaptationError::HttpClientRecvDataFailed(e)
            }
            H2StreamToChunkedTransferError::RecvTrailerFailed(e) => {
                H2ToH1ReqmodAdaptationError::HttpClientRecvTrailerFailed(e)
            }
        }
    }

    /// The client body is being written to the http upstream as chunked.
    pub(super) fn clt_to_upstream(e: H2StreamToChunkedTransferError) -> Self {
        match e {
            H2StreamToChunkedTransferError::WriteError(e) => {
                H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed(e)
            }
            H2StreamToChunkedTransferError::RecvDataFailed(e) => {
                H2ToH1ReqmodAdaptationError::HttpClientRecvDataFailed(e)
            }
            H2StreamToChunkedTransferError::RecvTrailerFailed(e) => {
                H2ToH1ReqmodAdaptationError::HttpClientRecvTrailerFailed(e)
            }
        }
    }
}

impl From<H2PreviewError> for H2ToH1ReqmodAdaptationError {
    fn from(value: H2PreviewError) -> Self {
        match value {
            H2PreviewError::ReadDataFailed(e) => {
                H2ToH1ReqmodAdaptationError::HttpClientRecvDataFailed(e)
            }
            H2PreviewError::ReadIdle => H2ToH1ReqmodAdaptationError::HttpClientReadIdle,
            H2PreviewError::IdleForceQuit(r) => H2ToH1ReqmodAdaptationError::IdleForceQuit(r),
        }
    }
}
