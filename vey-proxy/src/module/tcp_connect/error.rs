/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

use vey_http::connect::HttpConnectError;
use vey_http::upgrade::HttpUpgradeError;
use vey_resolver::ResolveError;
use vey_socks::SocksConnectError;
use vey_socks::v5::Socks5Reply;
use vey_types::net::{ConnectError, ProxyProtocolEncodeError};

use crate::serve::{ServerTaskError, ServerTaskForbiddenError};

#[derive(Debug, Error)]
pub(crate) enum UnderlyingTcpConnectError {
    #[error("resolve failed: {0}")]
    ResolveFailed(#[from] ResolveError),
    #[error("forbidden address family")]
    ForbiddenAddressFamily,
    #[error("forbidden remote address")]
    ForbiddenRemoteAddress,
    #[error("setup socket failed: {0:?}")]
    SetupSocketFailed(io::Error),
    #[error("escaper not usable: {0:?}")]
    EscaperNotUsable(anyhow::Error),
    #[error("connect failed: {0}")]
    ConnectFailed(#[from] ConnectError),
    #[error("timeout by rule")]
    TimeoutByRule,
    #[error("no address connected")]
    NoAddressConnected,
    #[error("proxy protocol encode error: {0}")]
    ProxyProtocolEncodeError(#[from] ProxyProtocolEncodeError),
    #[error("proxy protocol write failed: {0:?}")]
    ProxyProtocolWriteFailed(io::Error),
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
    #[error("internal tls client error: {0:?}")]
    InternalTlsClientError(anyhow::Error),
    #[error("peer tls handshake timeout")]
    PeerTlsHandshakeTimeout,
    #[error("peer tls handshake failed: {0:?}")]
    PeerTlsHandshakeFailed(anyhow::Error),
}

impl UnderlyingTcpConnectError {
    pub(crate) fn brief(&self) -> &'static str {
        match self {
            UnderlyingTcpConnectError::ResolveFailed(_) => "ResolveFailed",
            UnderlyingTcpConnectError::SetupSocketFailed(_) => "SetupSocketFailed",
            UnderlyingTcpConnectError::EscaperNotUsable(_) => "EscaperNotUsable",
            UnderlyingTcpConnectError::ConnectFailed(_) => "ConnectFailed",
            UnderlyingTcpConnectError::TimeoutByRule => "TimeoutByRule",
            UnderlyingTcpConnectError::NoAddressConnected => "NoAddressConnected",
            UnderlyingTcpConnectError::ForbiddenAddressFamily => "ForbiddenAddressFamily",
            UnderlyingTcpConnectError::ForbiddenRemoteAddress => "ForbiddenRemoteAddress",
            UnderlyingTcpConnectError::ProxyProtocolEncodeError(_) => "ProxyProtocolEncodeError",
            UnderlyingTcpConnectError::ProxyProtocolWriteFailed(_) => "ProxyProtocolWriteFailed",
            UnderlyingTcpConnectError::InternalServerError(_) => "InternalServerError",
            UnderlyingTcpConnectError::InternalTlsClientError(_) => "InternalTlsClientError",
            UnderlyingTcpConnectError::PeerTlsHandshakeTimeout => "PeerTLSHandshakeTimeout",
            UnderlyingTcpConnectError::PeerTlsHandshakeFailed(_) => "PeerTLSHandshakeFailed",
        }
    }
}

#[derive(Error, Debug)]
pub(crate) enum TcpConnectError {
    #[error("method is not available")]
    MethodUnavailable,
    #[error("escaper not usable: {0:?}")]
    EscaperNotUsable(anyhow::Error),
    #[error("resolve failed: {0}")]
    ResolveFailed(#[from] ResolveError),
    #[error("setup socket failed: {0:?}")]
    SetupSocketFailed(io::Error),
    #[error("connect failed: {0}")]
    ConnectFailed(#[from] ConnectError),
    #[error("timeout by rule")]
    TimeoutByRule,
    #[error("no address connected")]
    NoAddressConnected,
    #[error("forbidden address family")]
    ForbiddenAddressFamily,
    #[error("forbidden remote address")]
    ForbiddenRemoteAddress,
    #[error("proxy protocol encode error: {0}")]
    ProxyProtocolEncodeError(#[from] ProxyProtocolEncodeError),
    #[error("proxy protocol write failed: {0:?}")]
    ProxyProtocolWriteFailed(io::Error),
    #[error("negotiation read failed: {0:?}")]
    NegotiationReadFailed(io::Error),
    #[error("negotiation write failed: {0:?}")]
    NegotiationWriteFailed(io::Error),
    #[error("negotiation rejected: {0}")]
    NegotiationRejected(String),
    #[error("negotiation timeout")]
    NegotiationPeerTimeout,
    #[error("negotiation protocol error")]
    NegotiationProtocolErr,
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
    #[error("internal tls client error: {0:?}")]
    InternalTlsClientError(anyhow::Error),
    #[error("peer tls handshake timeout")]
    PeerTlsHandshakeTimeout,
    #[error("peer tls handshake failed: {0:?}")]
    PeerTlsHandshakeFailed(anyhow::Error),
    #[error("upstream tls handshake timeout")]
    UpstreamTlsHandshakeTimeout,
    #[error("upstream tls handshake failed: {0:?}")]
    UpstreamTlsHandshakeFailed(anyhow::Error),
}

impl From<TcpConnectError> for ServerTaskError {
    fn from(e: TcpConnectError) -> Self {
        match e {
            TcpConnectError::MethodUnavailable => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::MethodUnavailable)
            }
            TcpConnectError::EscaperNotUsable(e) => ServerTaskError::EscaperNotUsable(e),
            TcpConnectError::ResolveFailed(e) => ServerTaskError::from(e),
            TcpConnectError::SetupSocketFailed(_) => ServerTaskError::InternalServerError(
                "failed to setup local socket for remote connection",
            ),
            TcpConnectError::ConnectFailed(e) => ServerTaskError::UpstreamNotConnected(e),
            TcpConnectError::TimeoutByRule => {
                ServerTaskError::UpstreamNotConnected(ConnectError::TimedOut)
            }
            TcpConnectError::NoAddressConnected => ServerTaskError::UpstreamNotAvailable,
            TcpConnectError::ForbiddenAddressFamily | TcpConnectError::ForbiddenRemoteAddress => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::IpBlocked)
            }
            TcpConnectError::ProxyProtocolEncodeError(_) => {
                ServerTaskError::InternalServerError("proxy protocol encode failed")
            }
            TcpConnectError::ProxyProtocolWriteFailed(e) => ServerTaskError::UpstreamWriteFailed(e),
            TcpConnectError::NegotiationReadFailed(e) => ServerTaskError::UpstreamReadFailed(e),
            TcpConnectError::NegotiationWriteFailed(e) => ServerTaskError::UpstreamWriteFailed(e),
            TcpConnectError::NegotiationRejected(e) => ServerTaskError::UpstreamNotNegotiated(e),
            TcpConnectError::NegotiationPeerTimeout => {
                ServerTaskError::UpstreamAppTimeout("negotiation peer timeout")
            }
            TcpConnectError::NegotiationProtocolErr => {
                ServerTaskError::InvalidUpstreamProtocol("protocol negotiation with remote failed")
            }
            TcpConnectError::InternalServerError(s) => ServerTaskError::InternalServerError(s),
            TcpConnectError::InternalTlsClientError(e) => {
                ServerTaskError::InternalTlsClientError(e)
            }
            TcpConnectError::PeerTlsHandshakeTimeout => ServerTaskError::PeerTlsHandshakeTimeout,
            TcpConnectError::PeerTlsHandshakeFailed(e) => {
                ServerTaskError::PeerTlsHandshakeFailed(e)
            }
            TcpConnectError::UpstreamTlsHandshakeTimeout => {
                ServerTaskError::UpstreamTlsHandshakeTimeout
            }
            TcpConnectError::UpstreamTlsHandshakeFailed(e) => {
                ServerTaskError::UpstreamTlsHandshakeFailed(e)
            }
        }
    }
}

impl From<UnderlyingTcpConnectError> for TcpConnectError {
    fn from(e: UnderlyingTcpConnectError) -> Self {
        match e {
            UnderlyingTcpConnectError::ResolveFailed(e) => TcpConnectError::ResolveFailed(e),
            UnderlyingTcpConnectError::ForbiddenAddressFamily => {
                TcpConnectError::ForbiddenAddressFamily
            }
            UnderlyingTcpConnectError::ForbiddenRemoteAddress => {
                TcpConnectError::ForbiddenRemoteAddress
            }
            UnderlyingTcpConnectError::EscaperNotUsable(e) => TcpConnectError::EscaperNotUsable(e),
            UnderlyingTcpConnectError::SetupSocketFailed(e) => {
                TcpConnectError::SetupSocketFailed(e)
            }
            UnderlyingTcpConnectError::ConnectFailed(e) => TcpConnectError::ConnectFailed(e),
            UnderlyingTcpConnectError::TimeoutByRule => TcpConnectError::TimeoutByRule,
            UnderlyingTcpConnectError::NoAddressConnected => TcpConnectError::NoAddressConnected,
            UnderlyingTcpConnectError::ProxyProtocolEncodeError(e) => {
                TcpConnectError::ProxyProtocolEncodeError(e)
            }
            UnderlyingTcpConnectError::ProxyProtocolWriteFailed(e) => {
                TcpConnectError::ProxyProtocolWriteFailed(e)
            }
            UnderlyingTcpConnectError::InternalServerError(s) => {
                TcpConnectError::InternalServerError(s)
            }
            UnderlyingTcpConnectError::InternalTlsClientError(e) => {
                TcpConnectError::InternalTlsClientError(e)
            }
            UnderlyingTcpConnectError::PeerTlsHandshakeTimeout => {
                TcpConnectError::PeerTlsHandshakeTimeout
            }
            UnderlyingTcpConnectError::PeerTlsHandshakeFailed(e) => {
                TcpConnectError::PeerTlsHandshakeFailed(e)
            }
        }
    }
}

impl From<SocksConnectError> for TcpConnectError {
    fn from(e: SocksConnectError) -> Self {
        match e {
            SocksConnectError::ReadFailed(e) => TcpConnectError::NegotiationReadFailed(e),
            SocksConnectError::WriteFailed(e) => TcpConnectError::NegotiationWriteFailed(e),
            SocksConnectError::NoAuthMethodAvailable => {
                TcpConnectError::NegotiationRejected("no auth method".into())
            }
            SocksConnectError::UnsupportedAuthVersion => TcpConnectError::NegotiationRejected(
                "auth protocol mismatch with remote proxy".into(),
            ),
            SocksConnectError::AuthFailed => {
                TcpConnectError::NegotiationRejected("auth failed with remote proxy".into())
            }
            SocksConnectError::InvalidProtocol(_) => TcpConnectError::NegotiationProtocolErr,
            SocksConnectError::PeerTimeout => TcpConnectError::NegotiationPeerTimeout,
            SocksConnectError::RequestFailed(s) => TcpConnectError::NegotiationRejected(s),
        }
    }
}

impl From<&TcpConnectError> for Socks5Reply {
    fn from(e: &TcpConnectError) -> Self {
        match e {
            TcpConnectError::MethodUnavailable
            | TcpConnectError::ForbiddenAddressFamily
            | TcpConnectError::ForbiddenRemoteAddress => Socks5Reply::ForbiddenByRule,
            TcpConnectError::ConnectFailed(e) => match e {
                ConnectError::ConnectionRefused | ConnectError::ConnectionReset => {
                    Socks5Reply::ConnectionRefused
                }
                ConnectError::NetworkUnreachable => Socks5Reply::NetworkUnreachable,
                ConnectError::HostUnreachable => Socks5Reply::HostUnreachable,
                ConnectError::TimedOut => Socks5Reply::ConnectionTimedOut,
                ConnectError::UnspecifiedError(_) => Socks5Reply::GeneralServerFailure,
            },
            TcpConnectError::ResolveFailed(_) | TcpConnectError::NoAddressConnected => {
                Socks5Reply::HostUnreachable
            }
            TcpConnectError::TimeoutByRule => Socks5Reply::ConnectionTimedOut,
            TcpConnectError::EscaperNotUsable(_)
            | TcpConnectError::SetupSocketFailed(_)
            | TcpConnectError::ProxyProtocolEncodeError(_)
            | TcpConnectError::NegotiationProtocolErr => Socks5Reply::GeneralServerFailure,
            TcpConnectError::ProxyProtocolWriteFailed(_)
            | TcpConnectError::NegotiationReadFailed(_)
            | TcpConnectError::NegotiationWriteFailed(_) => Socks5Reply::GeneralServerFailure,
            TcpConnectError::NegotiationRejected(_) => Socks5Reply::ConnectionRefused,
            TcpConnectError::NegotiationPeerTimeout => Socks5Reply::ConnectionTimedOut,
            TcpConnectError::InternalServerError(_)
            | TcpConnectError::InternalTlsClientError(_) => Socks5Reply::GeneralServerFailure,
            TcpConnectError::PeerTlsHandshakeTimeout
            | TcpConnectError::PeerTlsHandshakeFailed(_) => Socks5Reply::GeneralServerFailure,
            TcpConnectError::UpstreamTlsHandshakeTimeout
            | TcpConnectError::UpstreamTlsHandshakeFailed(_) => Socks5Reply::GeneralServerFailure,
        }
    }
}

impl From<HttpConnectError> for TcpConnectError {
    fn from(e: HttpConnectError) -> Self {
        match e {
            HttpConnectError::RemoteClosed => TcpConnectError::NegotiationReadFailed(
                io::Error::new(io::ErrorKind::UnexpectedEof, "early eof"),
            ),
            HttpConnectError::ReadFailed(e) => TcpConnectError::NegotiationReadFailed(e),
            HttpConnectError::WriteFailed(e) => TcpConnectError::NegotiationWriteFailed(e),
            HttpConnectError::InvalidResponse(_) => TcpConnectError::NegotiationProtocolErr,
            HttpConnectError::UnexpectedStatusCode(code, reason) => {
                TcpConnectError::NegotiationRejected(format!(
                    "rejected by remote proxy with response {code} {reason}"
                ))
            }
            HttpConnectError::PeerTimeout(_) => TcpConnectError::NegotiationPeerTimeout,
        }
    }
}

impl From<HttpUpgradeError> for TcpConnectError {
    fn from(e: HttpUpgradeError) -> Self {
        match e {
            HttpUpgradeError::RemoteClosed => TcpConnectError::NegotiationReadFailed(
                io::Error::new(io::ErrorKind::UnexpectedEof, "early eof"),
            ),
            HttpUpgradeError::ReadFailed(e) => TcpConnectError::NegotiationReadFailed(e),
            HttpUpgradeError::WriteFailed(e) => TcpConnectError::NegotiationWriteFailed(e),
            HttpUpgradeError::InvalidResponse(_) => TcpConnectError::NegotiationProtocolErr,
            HttpUpgradeError::UnexpectedStatusCode(code, reason) => {
                TcpConnectError::NegotiationRejected(format!(
                    "rejected by remote proxy with response {code} {reason}"
                ))
            }
            HttpUpgradeError::PeerTimeout(_) => TcpConnectError::NegotiationPeerTimeout,
        }
    }
}
