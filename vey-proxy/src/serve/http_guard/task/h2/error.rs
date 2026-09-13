/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::anyhow;
use http::{Response, StatusCode, Version};
use thiserror::Error;

use vey_h2::H2StreamBodyTransferError;
use vey_icap_client::reqmod::h2::H2ReqmodAdaptationError;
use vey_icap_client::respmod::h2::H2RespmodAdaptationError;
use vey_io_ext::IdleForceQuitReason;

use crate::config::server::http_guard::HttpGuardServerConfig;
use crate::module::http_header::{self, ProxyErrorType};

#[derive(Debug, Error)]
pub(crate) enum H2StreamTransferError {
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
    #[error("internal adapter error: {0}")]
    InternalAdapterError(anyhow::Error),
    #[error("no matching site for Host")]
    SiteNotFound,
    #[error("Host does not match TLS SNI site")]
    MisdirectedRequest,
    #[error("failed to open origin connection: {0}")]
    OriginConnectFailed(anyhow::Error),
    #[error("failed to open upstream stream: {0}")]
    UpstreamStreamOpenFailed(h2::Error),
    #[error("timeout to open upstream stream")]
    UpstreamStreamOpenTimeout,
    #[error("failed to send request head: {0}")]
    RequestHeadSendFailed(h2::Error),
    #[error("invalid Host header")]
    InvalidHostHeader,
    #[error("failed to recv response head: {0}")]
    ResponseHeadRecvFailed(h2::Error),
    #[error("timeout to recv response head")]
    ResponseHeadRecvTimeout,
    #[error("invalid 100-continue response")]
    InvalidContinueResponse,
    #[error("unsupported informational response {0}")]
    UnsupportedInformationalResponse(StatusCode),
    #[error("failed to send response head: {0}")]
    ResponseHeadSendFailed(h2::Error),
    #[error("failed to transfer request body: {0}")]
    RequestBodyTransferFailed(H2StreamBodyTransferError),
    #[error("failed to transfer response body: {0}")]
    ResponseBodyTransferFailed(H2StreamBodyTransferError),
    #[error("canceled as user blocked")]
    CanceledAsUserBlocked,
    #[error("canceled as server quit")]
    CanceledAsServerQuit,
    #[error("idle after {0:?} x {1}")]
    Idle(Duration, usize),
}

impl H2StreamTransferError {
    pub(super) fn status_and_error(&self) -> Option<(StatusCode, ProxyErrorType)> {
        match self {
            H2StreamTransferError::InvalidHostHeader | H2StreamTransferError::SiteNotFound => {
                Some((StatusCode::BAD_REQUEST, ProxyErrorType::HttpRequestError))
            }
            H2StreamTransferError::MisdirectedRequest => Some((
                StatusCode::MISDIRECTED_REQUEST,
                ProxyErrorType::HttpRequestError,
            )),
            H2StreamTransferError::OriginConnectFailed(_) => Some((
                StatusCode::BAD_GATEWAY,
                ProxyErrorType::ConnectionTerminated,
            )),
            H2StreamTransferError::UpstreamStreamOpenFailed(_)
            | H2StreamTransferError::UpstreamStreamOpenTimeout => None,
            H2StreamTransferError::RequestHeadSendFailed(_) => Some((
                StatusCode::BAD_GATEWAY,
                ProxyErrorType::ConnectionTerminated,
            )),
            H2StreamTransferError::ResponseHeadRecvFailed(_) => Some((
                StatusCode::BAD_GATEWAY,
                ProxyErrorType::ConnectionTerminated,
            )),
            H2StreamTransferError::ResponseHeadRecvTimeout => Some((
                StatusCode::GATEWAY_TIMEOUT,
                ProxyErrorType::HttpResponseTimeout,
            )),
            H2StreamTransferError::InvalidContinueResponse
            | H2StreamTransferError::UnsupportedInformationalResponse(_) => {
                Some((StatusCode::BAD_GATEWAY, ProxyErrorType::HttpProtocolError))
            }
            _ => None,
        }
    }
}

pub(super) fn h2_local_error_response(
    config: &HttpGuardServerConfig,
    status: StatusCode,
    error: ProxyErrorType,
) -> Option<Response<()>> {
    if config.no_proxy_status {
        return Response::builder()
            .status(status)
            .version(Version::HTTP_2)
            .body(())
            .ok();
    }
    let ident = config
        .server_id
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or(http_header::DEFAULT_PROXY_STATUS_IDENT);
    Response::builder()
        .status(status)
        .version(Version::HTTP_2)
        .header(
            "proxy-status",
            http_header::proxy_status_value(ident, error),
        )
        .body(())
        .ok()
}

impl From<H2ReqmodAdaptationError> for H2StreamTransferError {
    fn from(e: H2ReqmodAdaptationError) -> Self {
        match e {
            H2ReqmodAdaptationError::InternalServerError(s) => {
                H2StreamTransferError::InternalServerError(s)
            }
            H2ReqmodAdaptationError::HttpClientRecvDataFailed(e) => {
                H2StreamTransferError::RequestBodyTransferFailed(
                    H2StreamBodyTransferError::RecvDataFailed(e),
                )
            }
            H2ReqmodAdaptationError::HttpClientRecvTrailerFailed(e) => {
                H2StreamTransferError::RequestBodyTransferFailed(
                    H2StreamBodyTransferError::RecvTrailersFailed(e),
                )
            }
            H2ReqmodAdaptationError::HttpUpstreamSendHeadFailed(e) => {
                H2StreamTransferError::RequestHeadSendFailed(e)
            }
            H2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e) => {
                H2StreamTransferError::RequestBodyTransferFailed(
                    H2StreamBodyTransferError::SendDataFailed(e),
                )
            }
            H2ReqmodAdaptationError::HttpUpstreamSendTrailedFailed(e) => {
                H2StreamTransferError::RequestBodyTransferFailed(
                    H2StreamBodyTransferError::SendTrailersFailed(e),
                )
            }
            H2ReqmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => H2StreamTransferError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => H2StreamTransferError::CanceledAsServerQuit,
            },
            H2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed(e) => {
                H2StreamTransferError::ResponseHeadRecvFailed(e)
            }
            H2ReqmodAdaptationError::HttpUpstreamRecvResponseTimeout => {
                H2StreamTransferError::ResponseHeadRecvTimeout
            }
            H2ReqmodAdaptationError::InvalidUpstreamContinueResponse => {
                H2StreamTransferError::InvalidContinueResponse
            }
            H2ReqmodAdaptationError::UnsupportedInformationalResponse(status) => {
                H2StreamTransferError::UnsupportedInformationalResponse(status)
            }
            H2ReqmodAdaptationError::HttpClientSendResponseFailed(e) => {
                H2StreamTransferError::ResponseHeadSendFailed(e)
            }
            e => H2StreamTransferError::InternalAdapterError(anyhow!("reqmod: {e}")),
        }
    }
}

impl From<H2RespmodAdaptationError> for H2StreamTransferError {
    fn from(e: H2RespmodAdaptationError) -> Self {
        match e {
            H2RespmodAdaptationError::InternalServerError(s) => {
                H2StreamTransferError::InternalServerError(s)
            }
            H2RespmodAdaptationError::HttpUpstreamRecvDataFailed(e) => {
                H2StreamTransferError::ResponseBodyTransferFailed(
                    H2StreamBodyTransferError::RecvDataFailed(e),
                )
            }
            H2RespmodAdaptationError::HttpUpstreamRecvTrailerFailed(e) => {
                H2StreamTransferError::ResponseBodyTransferFailed(
                    H2StreamBodyTransferError::RecvTrailersFailed(e),
                )
            }
            H2RespmodAdaptationError::HttpClientSendHeadFailed(e) => {
                H2StreamTransferError::ResponseHeadSendFailed(e)
            }
            H2RespmodAdaptationError::HttpClientSendDataFailed(e) => {
                H2StreamTransferError::ResponseBodyTransferFailed(
                    H2StreamBodyTransferError::SendDataFailed(e),
                )
            }
            H2RespmodAdaptationError::HttpClientSendTrailerFailed(e) => {
                H2StreamTransferError::ResponseBodyTransferFailed(
                    H2StreamBodyTransferError::SendTrailersFailed(e),
                )
            }
            H2RespmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => H2StreamTransferError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => H2StreamTransferError::CanceledAsServerQuit,
            },
            e => H2StreamTransferError::InternalAdapterError(anyhow!("respmod: {e}")),
        }
    }
}
