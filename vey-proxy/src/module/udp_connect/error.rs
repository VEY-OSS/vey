/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io;

use thiserror::Error;

use vey_http::upgrade::HttpUpgradeError;
use vey_resolver::ResolveError;
use vey_socks::SocksConnectError;
use vey_types::net::{ConnectError, ProxyProtocolEncodeError};

use crate::module::tcp_connect::UnderlyingTcpConnectError;
use crate::serve::{ServerTaskError, ServerTaskForbiddenError};

#[derive(Error, Debug)]
pub(crate) enum UdpConnectError {
    #[error("method is not available")]
    MethodUnavailable,
    #[error("escaper is not usable: {0:?}")]
    EscaperNotUsable(anyhow::Error),
    #[error("resolve failed: {0}")]
    ResolveFailed(#[from] ResolveError),
    #[error("forbidden address family")]
    ForbiddenAddressFamily,
    #[error("forbidden remote address")]
    ForbiddenRemoteAddress,
    #[error("setup socket failed: {0:?}")]
    SetupSocketFailed(io::Error),
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
    #[error("underlying connect timeout by rule")]
    UnderlyingTimeoutByRule,
    #[error("underlying tcp connect failed: {0:?}")]
    UnderlyingTcpConnectFailed(ConnectError),
    #[error("no address connected for underlying connection")]
    UnderlyingNoAddressConnected,
    #[error("internal server error: {0}")]
    InternalServerError(&'static str),
    #[error("internal tls client error: {0:?}")]
    InternalTlsClientError(anyhow::Error),
    #[error("peer tls handshake timeout")]
    PeerTlsHandshakeTimeout,
    #[error("peer tls handshake failed: {0:?}")]
    PeerTlsHandshakeFailed(anyhow::Error),
}

impl From<UnderlyingTcpConnectError> for UdpConnectError {
    fn from(e: UnderlyingTcpConnectError) -> Self {
        match e {
            UnderlyingTcpConnectError::ResolveFailed(e) => UdpConnectError::ResolveFailed(e),
            UnderlyingTcpConnectError::ForbiddenAddressFamily => {
                UdpConnectError::ForbiddenAddressFamily
            }
            UnderlyingTcpConnectError::ForbiddenRemoteAddress => {
                UdpConnectError::ForbiddenRemoteAddress
            }
            UnderlyingTcpConnectError::EscaperNotUsable(e) => UdpConnectError::EscaperNotUsable(e),
            UnderlyingTcpConnectError::SetupSocketFailed(e) => {
                UdpConnectError::SetupSocketFailed(e)
            }
            UnderlyingTcpConnectError::ConnectFailed(e) => {
                UdpConnectError::UnderlyingTcpConnectFailed(e)
            }
            UnderlyingTcpConnectError::TimeoutByRule => UdpConnectError::UnderlyingTimeoutByRule,
            UnderlyingTcpConnectError::NoAddressConnected => {
                UdpConnectError::UnderlyingNoAddressConnected
            }
            UnderlyingTcpConnectError::ProxyProtocolEncodeError(e) => {
                UdpConnectError::ProxyProtocolEncodeError(e)
            }
            UnderlyingTcpConnectError::ProxyProtocolWriteFailed(e) => {
                UdpConnectError::ProxyProtocolWriteFailed(e)
            }
            UnderlyingTcpConnectError::InternalServerError(s) => {
                UdpConnectError::InternalServerError(s)
            }
            UnderlyingTcpConnectError::InternalTlsClientError(e) => {
                UdpConnectError::InternalTlsClientError(e)
            }
            UnderlyingTcpConnectError::PeerTlsHandshakeTimeout => {
                UdpConnectError::PeerTlsHandshakeTimeout
            }
            UnderlyingTcpConnectError::PeerTlsHandshakeFailed(e) => {
                UdpConnectError::PeerTlsHandshakeFailed(e)
            }
        }
    }
}

impl From<SocksConnectError> for UdpConnectError {
    fn from(e: SocksConnectError) -> Self {
        match e {
            SocksConnectError::ReadFailed(e) => UdpConnectError::NegotiationReadFailed(e),
            SocksConnectError::WriteFailed(e) => UdpConnectError::NegotiationWriteFailed(e),
            SocksConnectError::NoAuthMethodAvailable => {
                UdpConnectError::NegotiationRejected("no auth method".into())
            }
            SocksConnectError::UnsupportedAuthVersion => UdpConnectError::NegotiationRejected(
                "auth protocol mismatch with remote proxy".into(),
            ),
            SocksConnectError::AuthFailed => {
                UdpConnectError::NegotiationRejected("auth failed with remote proxy".into())
            }
            SocksConnectError::InvalidProtocol(_) => UdpConnectError::NegotiationProtocolErr,
            SocksConnectError::PeerTimeout => UdpConnectError::NegotiationPeerTimeout,
            SocksConnectError::RequestFailed(s) => UdpConnectError::NegotiationRejected(s),
        }
    }
}

impl From<UdpConnectError> for ServerTaskError {
    fn from(e: UdpConnectError) -> Self {
        match e {
            UdpConnectError::MethodUnavailable => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::MethodUnavailable)
            }
            UdpConnectError::EscaperNotUsable(e) => ServerTaskError::EscaperNotUsable(e),
            UdpConnectError::ResolveFailed(e) => ServerTaskError::from(e),
            UdpConnectError::ForbiddenAddressFamily | UdpConnectError::ForbiddenRemoteAddress => {
                ServerTaskError::ForbiddenByRule(ServerTaskForbiddenError::IpBlocked)
            }
            UdpConnectError::SetupSocketFailed(_) => {
                ServerTaskError::InternalServerError("setup local udp socket failed")
            }
            UdpConnectError::ProxyProtocolEncodeError(_) => {
                ServerTaskError::InternalServerError("proxy protocol encode failed")
            }
            UdpConnectError::ProxyProtocolWriteFailed(e) => ServerTaskError::UpstreamWriteFailed(e),
            UdpConnectError::NegotiationReadFailed(e) => ServerTaskError::UpstreamReadFailed(e),
            UdpConnectError::NegotiationWriteFailed(e) => ServerTaskError::UpstreamWriteFailed(e),
            UdpConnectError::NegotiationRejected(e) => ServerTaskError::UpstreamNotNegotiated(e),
            UdpConnectError::NegotiationPeerTimeout => {
                ServerTaskError::UpstreamAppTimeout("negotiation peer timeout")
            }
            UdpConnectError::NegotiationProtocolErr => {
                ServerTaskError::InvalidUpstreamProtocol("protocol negotiation with remote failed")
            }
            UdpConnectError::UnderlyingTimeoutByRule => {
                ServerTaskError::UpstreamAppTimeout("underlying connect timeout by rule")
            }
            UdpConnectError::UnderlyingTcpConnectFailed(e) => {
                ServerTaskError::UpstreamNotConnected(e)
            }
            UdpConnectError::UnderlyingNoAddressConnected => ServerTaskError::UpstreamNotAvailable,
            UdpConnectError::InternalServerError(s) => ServerTaskError::InternalServerError(s),
            UdpConnectError::InternalTlsClientError(e) => {
                ServerTaskError::InternalTlsClientError(e)
            }
            UdpConnectError::PeerTlsHandshakeTimeout => ServerTaskError::PeerTlsHandshakeTimeout,
            UdpConnectError::PeerTlsHandshakeFailed(e) => {
                ServerTaskError::PeerTlsHandshakeFailed(e)
            }
        }
    }
}

impl From<HttpUpgradeError> for UdpConnectError {
    fn from(e: HttpUpgradeError) -> Self {
        match e {
            HttpUpgradeError::RemoteClosed => UdpConnectError::NegotiationReadFailed(
                io::Error::new(io::ErrorKind::UnexpectedEof, "early eof"),
            ),
            HttpUpgradeError::ReadFailed(e) => UdpConnectError::NegotiationReadFailed(e),
            HttpUpgradeError::WriteFailed(e) => UdpConnectError::NegotiationWriteFailed(e),
            HttpUpgradeError::InvalidResponse(_) => UdpConnectError::NegotiationProtocolErr,
            HttpUpgradeError::UnexpectedStatusCode(code, reason) => {
                UdpConnectError::NegotiationRejected(format!(
                    "rejected by remote proxy with response {code} {reason}"
                ))
            }
            HttpUpgradeError::PeerTimeout(_) => UdpConnectError::NegotiationPeerTimeout,
        }
    }
}
