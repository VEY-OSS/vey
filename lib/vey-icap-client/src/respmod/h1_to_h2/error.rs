/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

use vey_h2::{H2StreamBodyEncodeTransferError, H2StreamFromChunkedTransferError};
use vey_http::client::HttpResponseParseError;
use vey_io_ext::IdleForceQuitReason;

use crate::reason::IcapErrorReason;
use crate::respmod::IcapRespmodParseError;

#[derive(Debug, Error)]
pub enum H1ToH2RespmodAdaptationError {
    #[error("write to icap server failed: {0:?}")]
    IcapServerWriteFailed(io::Error),
    #[error("read from icap server failed: {0:?}")]
    IcapServerReadFailed(io::Error),
    #[error("connection closed by icap server")]
    IcapServerConnectionClosed,
    #[error("invalid response from icap server: {0}")]
    InvalidIcapServerResponse(#[from] IcapRespmodParseError),
    #[error("invalid http error response from icap server: {0}")]
    InvalidIcapServerHttpResponse(#[from] HttpResponseParseError),
    #[error("error response from icap server: {0} ({1} {2})")]
    IcapServerErrorResponse(IcapErrorReason, u16, String),
    #[error("read from http upstream failed: {0:?}")]
    HttpUpstreamReadFailed(io::Error),
    #[error("send head to http client failed: {0}")]
    HttpClientSendHeadFailed(h2::Error),
    #[error("client not in send state")]
    HttpClientNotInSendState,
    #[error("send data to http client failed: {0}")]
    HttpClientSendDataFailed(h2::Error),
    #[error("send trailer to http client failed: {0}")]
    HttpClientSendTrailerFailed(h2::Error),
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
    #[error("force quit from idle checker: {0:?}")]
    IdleForceQuit(IdleForceQuitReason),
    #[error("idle while reading from http upstream")]
    HttpUpstreamReadIdle,
    #[error("idle while writing to http client")]
    HttpClientWriteIdle,
    #[error("idle while reading from icap server")]
    IcapServerReadIdle,
    #[error("idle while writing to icap server")]
    IcapServerWriteIdle,
    #[error("not implemented feature: {0}")]
    NotImplemented(&'static str),
}

impl H1ToH2RespmodAdaptationError {
    /// The upstream chunked body is being sent to the http client directly.
    pub(super) fn ups_to_clt(e: H2StreamFromChunkedTransferError) -> Self {
        match e {
            H2StreamFromChunkedTransferError::ReadError(e) => {
                H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e)
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
}

/// The adapted chunked body is being read from the ICAP server.
impl From<H2StreamFromChunkedTransferError> for H1ToH2RespmodAdaptationError {
    fn from(e: H2StreamFromChunkedTransferError) -> Self {
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
}

impl From<H2StreamBodyEncodeTransferError> for H1ToH2RespmodAdaptationError {
    fn from(e: H2StreamBodyEncodeTransferError) -> Self {
        match e {
            H2StreamBodyEncodeTransferError::ReadError(e) => {
                H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e)
            }
            H2StreamBodyEncodeTransferError::SendDataFailed(e) => {
                H1ToH2RespmodAdaptationError::HttpClientSendDataFailed(e)
            }
            H2StreamBodyEncodeTransferError::SenderNotInSendState => {
                H1ToH2RespmodAdaptationError::HttpClientNotInSendState
            }
        }
    }
}
