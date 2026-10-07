/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;
use std::time::Duration;

use anyhow::anyhow;
use http::{StatusCode, Version};
use thiserror::Error;

use vey_dpi::Protocol;
use vey_ftp_client::FtpConnectError;
use vey_h2::{
    H2StreamBodyEncodeTransferError, H2StreamBodyTransferError, H2StreamFromChunkedTransferError,
    H2StreamToChunkedTransferError,
};
use vey_http::client::HttpResponseParseError;
use vey_http::server::HttpRequestParseError;
use vey_icap_client::reqmod::h1::H1ReqmodAdaptationError;
use vey_icap_client::reqmod::h2::H2ReqmodAdaptationError;
use vey_icap_client::reqmod::h2_to_h1::H2ToH1ReqmodAdaptationError;
use vey_icap_client::reqmod::imap::ImapAdaptationError;
use vey_icap_client::reqmod::smtp::SmtpAdaptationError;
use vey_icap_client::respmod::h1::H1RespmodAdaptationError;
use vey_icap_client::respmod::h1_to_h2::H1ToH2RespmodAdaptationError;
use vey_icap_client::respmod::h2::H2RespmodAdaptationError;
use vey_io_ext::{
    IdleForceQuitReason, UdpCopyClientError, UdpCopyRemoteError, UdpRelayClientError,
    UdpRelayError, UdpRelayRemoteError,
};
use vey_resolver::ResolveError;
use vey_socks::SocksRequestParseError;
use vey_types::net::ConnectError;

use crate::inspect::InterceptionError;
use crate::module::http_forward::HttpProxyClientResponse;
use crate::module::http_header::ProxyErrorType;
use crate::module::tcp_connect::TcpConnectError;

#[derive(Error, Debug)]
pub(crate) enum ServerTaskForbiddenError {
    #[error("method unavailable")]
    MethodUnavailable,
    #[error("client ip blocked")]
    ClientIpBlocked,
    #[error("request rate limited")]
    RateLimited,
    #[error("proxy request type banned")]
    ProtoBanned,
    #[error("target dest denied")]
    DestDenied,
    #[error("target ip blocked")]
    IpBlocked,
    #[error("fully loaded")]
    FullyLoaded,
    #[error("http ua blocked")]
    UaBlocked,
    #[error("user blocked")]
    UserBlocked,
}

#[derive(Error, Debug)]
pub(crate) enum ServerTaskH2Error {
    #[error("failed to open upstream h2 stream: {0}")]
    UpstreamStreamOpenFailed(h2::Error),
    #[error("timeout to open upstream h2 stream")]
    UpstreamStreamOpenTimeout,
}

impl ServerTaskH2Error {
    pub(crate) fn brief(&self) -> &'static str {
        match self {
            ServerTaskH2Error::UpstreamStreamOpenFailed(_) => "UpstreamH2StreamOpenFailed",
            ServerTaskH2Error::UpstreamStreamOpenTimeout => "UpstreamH2StreamOpenTimeout",
        }
    }
}

#[derive(Error, Debug)]
pub(crate) enum ServerTaskError {
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
    #[error("internal adapter error: {0:?}")]
    InternalAdapterError(anyhow::Error),
    #[error("internal resolver error: {0:?}")]
    InternalResolverError(ResolveError),
    #[error("internal tls client error: {0:?}")]
    InternalTlsClientError(anyhow::Error),
    #[error("peer tls handshake timeout")]
    PeerTlsHandshakeTimeout,
    #[error("peer tls handshake failed: {0:?}")]
    PeerTlsHandshakeFailed(anyhow::Error),
    #[error("escaper not usable: {0:?}")]
    EscaperNotUsable(anyhow::Error),
    #[error("forbidden by rule: {0}")]
    ForbiddenByRule(#[from] ServerTaskForbiddenError),
    #[error("invalid client protocol: {0}")]
    InvalidClientProtocol(&'static str),
    #[error("unimplemented protocol")]
    UnimplementedProtocol,
    #[error("tcp read from client: {0:?}")]
    ClientTcpReadFailed(io::Error),
    #[error("tcp write to client: {0:?}")]
    ClientTcpWriteFailed(io::Error),
    #[error("udp recv from client: {0:?}")]
    ClientUdpRecvFailed(io::Error),
    #[error("udp send to client: {0:?}")]
    ClientUdpSendFailed(io::Error),
    #[error("client authentication failed")]
    ClientAuthFailed,
    #[error("client app timeout: {0}")]
    ClientAppTimeout(&'static str),
    #[error("client app error: {0:?}")]
    ClientAppError(anyhow::Error), // may contain client app timeout error
    #[error("upstream not resolved: {0}")]
    UpstreamNotResolved(ResolveError),
    #[error("upstream not connected: {0}")]
    UpstreamNotConnected(ConnectError),
    #[error("upstream not available")]
    UpstreamNotAvailable,
    #[error("invalid upstream protocol: {0}")]
    InvalidUpstreamProtocol(&'static str),
    #[error("read from upstream: {0:?}")]
    UpstreamReadFailed(io::Error),
    #[error("write to upstream: {0:?}")]
    UpstreamWriteFailed(io::Error),
    #[error("upstream tls handshake timeout")]
    UpstreamTlsHandshakeTimeout,
    #[error("upstream tls handshake failed: {0:?}")]
    UpstreamTlsHandshakeFailed(anyhow::Error),
    #[error("upstream not negotiated: {0}")]
    UpstreamNotNegotiated(String),
    #[error("upstream app unavailable")]
    UpstreamAppUnavailable,
    #[error("upstream app timeout: {0}")]
    UpstreamAppTimeout(&'static str),
    #[error("upstream app error: {0:?}")]
    UpstreamAppError(anyhow::Error), // may contain upstream app timeout error
    #[error("{0}")]
    H2(#[from] ServerTaskH2Error),
    #[error("closed by upstream")]
    ClosedByUpstream,
    #[error("closed by client")]
    ClosedByClient,
    #[error("closed early by client")]
    ClosedEarlyByClient,
    #[error("canceled as user blocked")]
    CanceledAsUserBlocked,
    #[error("canceled as server quit")]
    CanceledAsServerQuit,
    #[error("idle after {0:?} x {1}")]
    Idle(Duration, usize),
    #[error("{0} interception error: {1}")]
    InterceptionError(Protocol, InterceptionError),
    #[error("finished")]
    Finished, // this isn't an error, for log only
    #[error("unclassified error: {0:?}")]
    UnclassifiedError(#[from] anyhow::Error),
}

impl ServerTaskError {
    pub(crate) fn brief(&self) -> &'static str {
        match self {
            ServerTaskError::InternalServerError(_) => "InternalServerError",
            ServerTaskError::InternalAdapterError(_) => "InternalAdapterError",
            ServerTaskError::InternalResolverError(_) => "InternalResolverError",
            ServerTaskError::InternalTlsClientError(_) => "InternalTlsClientError",
            ServerTaskError::PeerTlsHandshakeTimeout => "PeerTlsHandshakeTimeout",
            ServerTaskError::PeerTlsHandshakeFailed(_) => "PeerTlsHandshakeFailed",
            ServerTaskError::EscaperNotUsable(_) => "EscaperNotUsable",
            ServerTaskError::ForbiddenByRule(_) => "ForbiddenByRule",
            ServerTaskError::InvalidClientProtocol(_) => "InvalidClientProtocol",
            ServerTaskError::UnimplementedProtocol => "UnimplementedProtocol",
            ServerTaskError::ClientTcpReadFailed(_) => "ClientTcpReadFailed",
            ServerTaskError::ClientTcpWriteFailed(_) => "ClientTcpWriteFailed",
            ServerTaskError::ClientUdpRecvFailed(_) => "ClientUdpRecvFailed",
            ServerTaskError::ClientUdpSendFailed(_) => "ClientUdpSendFailed",
            ServerTaskError::ClientAuthFailed => "ClientAuthFailed",
            ServerTaskError::ClientAppTimeout(_) => "ClientAppTimeout",
            ServerTaskError::ClientAppError(_) => "ClientAppError",
            ServerTaskError::UpstreamNotResolved(_) => "UpstreamNotResolved",
            ServerTaskError::UpstreamNotConnected(_) => "UpstreamNotConnected",
            ServerTaskError::UpstreamNotAvailable => "UpstreamNotAvailable",
            ServerTaskError::InvalidUpstreamProtocol(_) => "InvalidUpstreamProtocol",
            ServerTaskError::UpstreamReadFailed(_) => "UpstreamReadFailed",
            ServerTaskError::UpstreamWriteFailed(_) => "UpstreamWriteFailed",
            ServerTaskError::UpstreamTlsHandshakeTimeout => "UpstreamTlsHandshakeTimeout",
            ServerTaskError::UpstreamTlsHandshakeFailed(_) => "UpstreamTlsHandshakeFailed",
            ServerTaskError::UpstreamNotNegotiated(_) => "UpstreamNotNegotiated",
            ServerTaskError::UpstreamAppUnavailable => "UpstreamAppUnavailable",
            ServerTaskError::UpstreamAppTimeout(_) => "UpstreamAppTimeout",
            ServerTaskError::UpstreamAppError(_) => "UpstreamAppError",
            ServerTaskError::H2(e) => e.brief(),
            ServerTaskError::ClosedByUpstream => "ClosedByUpstream",
            ServerTaskError::ClosedByClient => "ClosedByClient",
            ServerTaskError::ClosedEarlyByClient => "ClosedEarlyByClient",
            ServerTaskError::CanceledAsUserBlocked => "CanceledAsUserBlocked",
            ServerTaskError::CanceledAsServerQuit => "CanceledAsServerQuit",
            ServerTaskError::Idle(_, _) => "Idle",
            ServerTaskError::InterceptionError(_, _) => "InterceptionError",
            ServerTaskError::Finished => "Finished",
            ServerTaskError::UnclassifiedError(_) => "UnclassifiedError",
        }
    }

    pub(crate) fn invalid_upstream_100_continue_response() -> Self {
        ServerTaskError::UpstreamAppError(anyhow!("invalid 100-continue response"))
    }

    pub(crate) fn reply_status(&self) -> Option<(StatusCode, ProxyErrorType)> {
        let rsp = HttpProxyClientResponse::from_task_err(self, Version::HTTP_2, true)?;
        Some((rsp.status_code(), rsp.proxy_error()?))
    }

    pub(crate) fn request_h2_body_error(e: H2StreamBodyTransferError) -> Self {
        match e {
            H2StreamBodyTransferError::RecvDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 data from client failed: {e}"))
            }
            H2StreamBodyTransferError::RecvTrailersFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 trailer from client failed: {e}"))
            }
            H2StreamBodyTransferError::ReleaseRecvCapacityFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!(
                    "release h2 recv capacity from client failed: {e}"
                ))
            }
            H2StreamBodyTransferError::WaitSendCapacityFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!(
                    "wait h2 send capacity to upstream failed: {e}"
                ))
            }
            H2StreamBodyTransferError::SendDataFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!("send h2 data to upstream failed: {e}"))
            }
            H2StreamBodyTransferError::SendTrailersFailed(e) => ServerTaskError::UpstreamAppError(
                anyhow!("send h2 trailer to upstream failed: {e}"),
            ),
            H2StreamBodyTransferError::GracefulCloseError(e) => {
                ServerTaskError::UpstreamAppError(anyhow!("close h2 upstream stream failed: {e}"))
            }
            H2StreamBodyTransferError::SenderNotInSendState => {
                ServerTaskError::UpstreamAppError(anyhow!("h2 upstream sender not in send state"))
            }
        }
    }

    pub(crate) fn response_h2_body_error(e: H2StreamBodyTransferError) -> Self {
        match e {
            H2StreamBodyTransferError::RecvDataFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!("recv h2 data from upstream failed: {e}"))
            }
            H2StreamBodyTransferError::RecvTrailersFailed(e) => ServerTaskError::UpstreamAppError(
                anyhow!("recv h2 trailer from upstream failed: {e}"),
            ),
            H2StreamBodyTransferError::ReleaseRecvCapacityFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!(
                    "release h2 recv capacity from upstream failed: {e}"
                ))
            }
            H2StreamBodyTransferError::WaitSendCapacityFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!(
                    "wait h2 send capacity to client failed: {e}"
                ))
            }
            H2StreamBodyTransferError::SendDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 data to client failed: {e}"))
            }
            H2StreamBodyTransferError::SendTrailersFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 trailer to client failed: {e}"))
            }
            H2StreamBodyTransferError::GracefulCloseError(e) => {
                ServerTaskError::ClientAppError(anyhow!("close h2 client stream failed: {e}"))
            }
            H2StreamBodyTransferError::SenderNotInSendState => {
                ServerTaskError::ClientAppError(anyhow!("h2 client sender not in send state"))
            }
        }
    }

    pub(crate) fn request_h2_to_chunked_error(e: H2StreamToChunkedTransferError) -> Self {
        match e {
            H2StreamToChunkedTransferError::WriteError(e) => {
                ServerTaskError::UpstreamWriteFailed(e)
            }
            H2StreamToChunkedTransferError::RecvDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 data from client failed: {e}"))
            }
            H2StreamToChunkedTransferError::RecvTrailerFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 trailer from client failed: {e}"))
            }
        }
    }

    pub(crate) fn response_chunked_to_h2_error(e: H2StreamFromChunkedTransferError) -> Self {
        match e {
            H2StreamFromChunkedTransferError::ReadError(e) => {
                ServerTaskError::UpstreamReadFailed(e)
            }
            H2StreamFromChunkedTransferError::SendDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 data to client failed: {e}"))
            }
            H2StreamFromChunkedTransferError::SendTrailerFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 trailer to client failed: {e}"))
            }
            H2StreamFromChunkedTransferError::SenderNotInSendState => {
                ServerTaskError::ClientAppError(anyhow!("h2 client sender not in send state"))
            }
        }
    }

    pub(crate) fn response_h2_encode_error(e: H2StreamBodyEncodeTransferError) -> Self {
        match e {
            H2StreamBodyEncodeTransferError::ReadError(e) => ServerTaskError::UpstreamReadFailed(e),
            H2StreamBodyEncodeTransferError::SendDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 data to client failed: {e}"))
            }
            H2StreamBodyEncodeTransferError::SenderNotInSendState => {
                ServerTaskError::ClientAppError(anyhow!("h2 client sender not in send state"))
            }
        }
    }
}

pub(crate) type ServerTaskResult<T> = Result<T, ServerTaskError>;

impl From<ResolveError> for ServerTaskError {
    fn from(e: ResolveError) -> Self {
        if matches!(e, ResolveError::ServerError(_)) {
            ServerTaskError::UpstreamNotResolved(e)
        } else {
            ServerTaskError::InternalResolverError(e)
        }
    }
}

impl From<UdpRelayClientError> for ServerTaskError {
    fn from(e: UdpRelayClientError) -> Self {
        match e {
            UdpRelayClientError::RecvFailed(e) => ServerTaskError::ClientUdpRecvFailed(e),
            UdpRelayClientError::SendFailed(e) => ServerTaskError::ClientUdpSendFailed(e),
            UdpRelayClientError::InvalidPacket(_) => {
                ServerTaskError::InvalidClientProtocol("invalid udp packet from client")
            }
            UdpRelayClientError::AddressNotSupported => ServerTaskError::UnimplementedProtocol,
            UdpRelayClientError::MismatchedClientAddress
            | UdpRelayClientError::ForbiddenClientAddress => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::ClientIpBlocked)
            }
            UdpRelayClientError::ForbiddenTargetAddress => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::DestDenied)
            }
        }
    }
}

impl From<UdpRelayRemoteError> for ServerTaskError {
    fn from(e: UdpRelayRemoteError) -> Self {
        match e {
            UdpRelayRemoteError::NoListenSocket => {
                ServerTaskError::InternalServerError("no running udp listen socket at remote side")
            }
            UdpRelayRemoteError::RecvFailed(_, e) => ServerTaskError::UpstreamReadFailed(e),
            UdpRelayRemoteError::SendFailed(_, _, e) => ServerTaskError::UpstreamWriteFailed(e),
            UdpRelayRemoteError::BatchSendFailed(_, e) => ServerTaskError::UpstreamWriteFailed(e),
            UdpRelayRemoteError::InvalidPacket(_, _) => {
                ServerTaskError::InvalidUpstreamProtocol("invalid received udp packet")
            }
            UdpRelayRemoteError::AddressNotSupported => ServerTaskError::UnimplementedProtocol,
            UdpRelayRemoteError::DomainNotResolved(e) => ServerTaskError::from(e),
            UdpRelayRemoteError::ForbiddenTargetIpAddress(_) => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::IpBlocked)
            }
            UdpRelayRemoteError::RemoteSessionClosed(_, _) => ServerTaskError::ClosedByUpstream,
            UdpRelayRemoteError::RemoteSessionError(_, _, e) => {
                ServerTaskError::UpstreamReadFailed(e)
            }
            UdpRelayRemoteError::InternalServerError(s) => ServerTaskError::InternalServerError(s),
        }
    }
}

impl From<UdpRelayError> for ServerTaskError {
    fn from(e: UdpRelayError) -> Self {
        match e {
            UdpRelayError::ClientError(e) => ServerTaskError::from(e),
            UdpRelayError::RemoteError(_, e) => ServerTaskError::from(e),
        }
    }
}

impl From<UdpCopyClientError> for ServerTaskError {
    fn from(e: UdpCopyClientError) -> Self {
        match e {
            UdpCopyClientError::RecvFailed(e) => ServerTaskError::ClientUdpRecvFailed(e),
            UdpCopyClientError::SendFailed(e) => ServerTaskError::ClientUdpSendFailed(e),
            UdpCopyClientError::InvalidPacket(_) => {
                ServerTaskError::InvalidClientProtocol("invalid udp packet from client")
            }
            UdpCopyClientError::MismatchedClientAddress
            | UdpCopyClientError::ForbiddenClientAddress => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::ClientIpBlocked)
            }
            UdpCopyClientError::VaryUpstream => {
                ServerTaskError::InvalidClientProtocol("vary upstream for udp connect")
            }
        }
    }
}

impl From<UdpCopyRemoteError> for ServerTaskError {
    fn from(e: UdpCopyRemoteError) -> Self {
        match e {
            UdpCopyRemoteError::RecvFailed(e) => ServerTaskError::UpstreamReadFailed(e),
            UdpCopyRemoteError::SendFailed(e) => ServerTaskError::UpstreamWriteFailed(e),
            UdpCopyRemoteError::InvalidPacket(_) => {
                ServerTaskError::InvalidUpstreamProtocol("invalid received udp packet")
            }
            UdpCopyRemoteError::RemoteSessionClosed => ServerTaskError::ClosedByUpstream,
            UdpCopyRemoteError::RemoteSessionError(e) => ServerTaskError::UpstreamReadFailed(e),
            UdpCopyRemoteError::InternalServerError(s) => ServerTaskError::InternalServerError(s),
        }
    }
}

impl From<HttpRequestParseError> for ServerTaskError {
    fn from(e: HttpRequestParseError) -> ServerTaskError {
        match e {
            HttpRequestParseError::ClientClosed => ServerTaskError::ClosedEarlyByClient,
            HttpRequestParseError::TooLargeHeader(_) => {
                ServerTaskError::InvalidClientProtocol("too large header in client request")
            }
            HttpRequestParseError::InvalidUpgradeRequest
            | HttpRequestParseError::UnsupportedMethod(_)
            | HttpRequestParseError::UnsupportedScheme
            | HttpRequestParseError::UnsupportedTransferEncoding => {
                ServerTaskError::UnimplementedProtocol
            }
            HttpRequestParseError::IoFailed(e) => ServerTaskError::ClientTcpReadFailed(e),
            HttpRequestParseError::UnmatchedHostAndAuthority => {
                ServerTaskError::InvalidClientProtocol("host header doesn't match host in uri")
            }
            _ => ServerTaskError::InvalidClientProtocol("invalid client request"),
        }
    }
}

impl From<HttpResponseParseError> for ServerTaskError {
    fn from(e: HttpResponseParseError) -> ServerTaskError {
        match e {
            HttpResponseParseError::RemoteClosed => ServerTaskError::ClosedByUpstream,
            HttpResponseParseError::TooLargeHeader(_) => {
                ServerTaskError::InvalidUpstreamProtocol("too large header in remote response")
            }
            HttpResponseParseError::IoFailed(e) => ServerTaskError::UpstreamReadFailed(e),
            _ => ServerTaskError::InvalidUpstreamProtocol("invalid remote response"),
        }
    }
}

impl From<FtpConnectError<TcpConnectError>> for ServerTaskError {
    fn from(e: FtpConnectError<TcpConnectError>) -> Self {
        match e {
            FtpConnectError::ConnectIoError(e) => ServerTaskError::from(e),
            FtpConnectError::ConnectTimedOut => {
                ServerTaskError::UpstreamAppTimeout("ftp connect timed out")
            }
            FtpConnectError::GreetingTimedOut => {
                ServerTaskError::UpstreamAppTimeout("ftp greeting timed out")
            }
            FtpConnectError::GreetingFailed(_)
            | FtpConnectError::NegotiationFailed(_)
            | FtpConnectError::InvalidReplyCode(_)
            | FtpConnectError::ServiceNotAvailable => {
                ServerTaskError::UpstreamNotNegotiated(format!("ftp connect failed: {e}"))
            }
        }
    }
}

impl From<SocksRequestParseError> for ServerTaskError {
    fn from(e: SocksRequestParseError) -> Self {
        match e {
            SocksRequestParseError::ReadFailed(e) => ServerTaskError::ClientTcpReadFailed(e),
            SocksRequestParseError::InvalidProtocol(_) => {
                ServerTaskError::InvalidClientProtocol("invalid socks protocol")
            }
            SocksRequestParseError::InvalidUdpPeerAddress => {
                ServerTaskError::InvalidClientProtocol(
                    "invalid udp peer address in negotiation stage",
                )
            }
            SocksRequestParseError::ClientClosed => ServerTaskError::ClosedEarlyByClient,
        }
    }
}

impl From<H1ReqmodAdaptationError> for ServerTaskError {
    fn from(e: H1ReqmodAdaptationError) -> Self {
        match e {
            H1ReqmodAdaptationError::InternalServerError(s) => {
                ServerTaskError::InternalServerError(s)
            }
            H1ReqmodAdaptationError::HttpClientReadFailed(e) => {
                ServerTaskError::ClientTcpReadFailed(e)
            }
            H1ReqmodAdaptationError::InvalidHttpClientRequestBody => {
                ServerTaskError::InvalidClientProtocol("invalid http body in client request")
            }
            H1ReqmodAdaptationError::HttpUpstreamWriteFailed(e) => {
                ServerTaskError::UpstreamWriteFailed(e)
            }
            H1ReqmodAdaptationError::HttpClientReadIdle => {
                ServerTaskError::ClientAppTimeout("idle while reading")
            }
            H1ReqmodAdaptationError::HttpUpstreamWriteIdle => {
                ServerTaskError::UpstreamAppTimeout("idle while writing")
            }
            H1ReqmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("reqmod: {e}")),
        }
    }
}

impl From<H1RespmodAdaptationError> for ServerTaskError {
    fn from(e: H1RespmodAdaptationError) -> Self {
        match e {
            H1RespmodAdaptationError::InternalServerError(s) => {
                ServerTaskError::InternalServerError(s)
            }
            H1RespmodAdaptationError::HttpUpstreamReadFailed(e) => {
                ServerTaskError::UpstreamReadFailed(e)
            }
            H1RespmodAdaptationError::InvalidHttpUpstreamResponseBody => {
                ServerTaskError::InvalidUpstreamProtocol("invalid http body in upstream response")
            }
            H1RespmodAdaptationError::HttpClientWriteFailed(e) => {
                ServerTaskError::ClientTcpWriteFailed(e)
            }
            H1RespmodAdaptationError::HttpUpstreamReadIdle => {
                ServerTaskError::UpstreamAppTimeout("idle while reading")
            }
            H1RespmodAdaptationError::HttpClientWriteIdle => {
                ServerTaskError::ClientAppTimeout("idle while writing")
            }
            H1RespmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("respmod: {e}")),
        }
    }
}

impl From<H2ReqmodAdaptationError> for ServerTaskError {
    fn from(e: H2ReqmodAdaptationError) -> Self {
        match e {
            H2ReqmodAdaptationError::InternalServerError(s) => {
                ServerTaskError::InternalServerError(s)
            }
            H2ReqmodAdaptationError::HttpClientRecvDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 data from client failed: {e}"))
            }
            H2ReqmodAdaptationError::HttpClientRecvTrailerFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 trailer from client failed: {e}"))
            }
            H2ReqmodAdaptationError::HttpUpstreamSendHeadFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!("send h2 head to upstream failed: {e}"))
            }
            H2ReqmodAdaptationError::HttpUpstreamSendDataFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!("send h2 data to upstream failed: {e}"))
            }
            H2ReqmodAdaptationError::HttpUpstreamSendTrailedFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!(
                    "send h2 trailer to upstream failed: {e}"
                ))
            }
            H2ReqmodAdaptationError::HttpUpstreamRecvResponseFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!(
                    "recv h2 response from upstream failed: {e}"
                ))
            }
            H2ReqmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            H2ReqmodAdaptationError::HttpUpstreamRecvResponseTimeout => {
                ServerTaskError::UpstreamAppTimeout("timeout to recv response head")
            }
            H2ReqmodAdaptationError::InvalidUpstreamContinueResponse => {
                ServerTaskError::invalid_upstream_100_continue_response()
            }
            H2ReqmodAdaptationError::UnsupportedInformationalResponse(status) => {
                ServerTaskError::UpstreamAppError(anyhow!(
                    "unsupported h2 informational response {status}"
                ))
            }
            H2ReqmodAdaptationError::HttpClientSendResponseFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 response to client failed: {e}"))
            }
            e => ServerTaskError::InternalAdapterError(anyhow!("reqmod: {e}")),
        }
    }
}

impl From<H2RespmodAdaptationError> for ServerTaskError {
    fn from(e: H2RespmodAdaptationError) -> Self {
        match e {
            H2RespmodAdaptationError::InternalServerError(s) => {
                ServerTaskError::InternalServerError(s)
            }
            H2RespmodAdaptationError::HttpUpstreamRecvDataFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!("recv h2 data from upstream failed: {e}"))
            }
            H2RespmodAdaptationError::HttpUpstreamRecvTrailerFailed(e) => {
                ServerTaskError::UpstreamAppError(anyhow!(
                    "recv h2 trailer from upstream failed: {e}"
                ))
            }
            H2RespmodAdaptationError::HttpClientSendHeadFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 head to client failed: {e}"))
            }
            H2RespmodAdaptationError::HttpClientSendDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 data to client failed: {e}"))
            }
            H2RespmodAdaptationError::HttpClientSendTrailerFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 trailer to client failed: {e}"))
            }
            H2RespmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("respmod: {e}")),
        }
    }
}

impl From<H2ToH1ReqmodAdaptationError> for ServerTaskError {
    fn from(e: H2ToH1ReqmodAdaptationError) -> Self {
        match e {
            H2ToH1ReqmodAdaptationError::HttpClientRecvDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 data from client failed: {e}"))
            }
            H2ToH1ReqmodAdaptationError::HttpClientRecvTrailerFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("recv h2 trailer from client failed: {e}"))
            }
            H2ToH1ReqmodAdaptationError::HttpUpstreamWriteFailed(e) => {
                ServerTaskError::UpstreamWriteFailed(e)
            }
            H2ToH1ReqmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("reqmod h2-to-h1: {e}")),
        }
    }
}

impl From<H1ToH2RespmodAdaptationError> for ServerTaskError {
    fn from(e: H1ToH2RespmodAdaptationError) -> Self {
        match e {
            H1ToH2RespmodAdaptationError::InternalServerError(s) => {
                ServerTaskError::InternalServerError(s)
            }
            H1ToH2RespmodAdaptationError::HttpUpstreamReadFailed(e) => {
                ServerTaskError::UpstreamReadFailed(e)
            }
            H1ToH2RespmodAdaptationError::HttpClientSendHeadFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 head to client failed: {e}"))
            }
            H1ToH2RespmodAdaptationError::HttpClientSendDataFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 data to client failed: {e}"))
            }
            H1ToH2RespmodAdaptationError::HttpClientSendTrailerFailed(e) => {
                ServerTaskError::ClientAppError(anyhow!("send h2 trailer to client failed: {e}"))
            }
            H1ToH2RespmodAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("respmod h1-to-h2: {e}")),
        }
    }
}

impl From<SmtpAdaptationError> for ServerTaskError {
    fn from(e: SmtpAdaptationError) -> Self {
        match e {
            SmtpAdaptationError::InternalServerError(s) => ServerTaskError::InternalServerError(s),
            SmtpAdaptationError::SmtpClientReadFailed(e) => ServerTaskError::ClientTcpReadFailed(e),
            SmtpAdaptationError::InvalidSmtpClientMessage => {
                ServerTaskError::InvalidClientProtocol("invalid smtp message from client")
            }
            SmtpAdaptationError::SmtpUpstreamWriteFailed(e) => {
                ServerTaskError::UpstreamWriteFailed(e)
            }
            SmtpAdaptationError::SmtpClientReadIdle => {
                ServerTaskError::ClientAppTimeout("idle while reading smtp mail message")
            }
            SmtpAdaptationError::SmtpUpstreamWriteIdle => {
                ServerTaskError::UpstreamAppTimeout("idle while writing smtp mail message")
            }
            SmtpAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("reqmod: {e}")),
        }
    }
}

impl From<ImapAdaptationError> for ServerTaskError {
    fn from(e: ImapAdaptationError) -> Self {
        match e {
            ImapAdaptationError::InternalServerError(s) => ServerTaskError::InternalServerError(s),
            ImapAdaptationError::ImapClientReadFailed(e) => ServerTaskError::ClientTcpReadFailed(e),
            ImapAdaptationError::ImapUpstreamWriteFailed(e) => {
                ServerTaskError::UpstreamWriteFailed(e)
            }
            ImapAdaptationError::ImapClientReadIdle => {
                ServerTaskError::ClientAppTimeout("idle while reading imap mail message")
            }
            ImapAdaptationError::ImapUpstreamWriteIdle => {
                ServerTaskError::UpstreamAppTimeout("idle while writing imap mail message")
            }
            ImapAdaptationError::IdleForceQuit(reason) => match reason {
                IdleForceQuitReason::UserBlocked => ServerTaskError::CanceledAsUserBlocked,
                IdleForceQuitReason::ServerQuit => ServerTaskError::CanceledAsServerQuit,
            },
            e => ServerTaskError::InternalAdapterError(anyhow!("reqmod: {e}")),
        }
    }
}
