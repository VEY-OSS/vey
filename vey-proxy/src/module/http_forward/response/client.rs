/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::io::{self, Write};
use std::net::{IpAddr, SocketAddr};

use http::{StatusCode, Version};
use mime::Mime;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use vey_ftp_client::FtpConnectError;
use vey_http::server::HttpRequestParseError;
use vey_io_ext::LimitedWriteExt;
use vey_resolver::ResolveError;
use vey_types::net::{ConnectError, HttpServerId};

use crate::module::http_header::{self, ProxyErrorType};
use crate::module::tcp_connect::TcpConnectError;
use crate::module::udp_connect::UdpConnectError;
use crate::serve::ServerTaskError;

struct CustomStatusCode {}

impl CustomStatusCode {
    const WEB_SERVER_IS_DOWN: u16 = 521;
    const CONNECTION_TIMED_OUT: u16 = 522;
    const ORIGIN_IS_UNREACHABLE: u16 = 523;
    const SSL_HANDSHAKE_FAILED: u16 = 525;
    const ORIGIN_DNS_ERROR: u16 = 530;

    fn canonical_reason(code: u16) -> &'static str {
        match code {
            Self::WEB_SERVER_IS_DOWN => "Web Server Is Down",
            Self::CONNECTION_TIMED_OUT => "Connection Timed Out",
            Self::ORIGIN_IS_UNREACHABLE => "Origin Is Unreachable",
            Self::ORIGIN_DNS_ERROR => "Origin DNS Error",
            Self::SSL_HANDSHAKE_FAILED => "SSL Handshake Failed",
            _ => "<unknown status code>",
        }
    }
}

pub(crate) struct HttpProxyClientResponse {
    status: StatusCode,
    version: Version,
    close: bool,
    extra_headers: Vec<String>,
    custom_error_message: Option<&'static str>,
    proxy_error: Option<ProxyErrorType>,
    proxy_status_ident: Option<HttpServerId>,
    omit_proxy_status: bool,
}

impl HttpProxyClientResponse {
    const RESPONSE_BUFFER_SIZE: usize = 1024;

    pub(crate) fn status(&self) -> u16 {
        self.status.as_u16()
    }

    pub(crate) fn from_standard(status: StatusCode, version: Version, close: bool) -> Self {
        HttpProxyClientResponse {
            status,
            version,
            close,
            extra_headers: Vec::new(),
            custom_error_message: None,
            proxy_error: None,
            proxy_status_ident: None,
            omit_proxy_status: false,
        }
    }

    fn with_proxy_error(mut self, error: ProxyErrorType) -> Self {
        self.proxy_error = Some(error);
        self
    }

    pub(crate) fn set_proxy_status_ident(&mut self, ident: &HttpServerId) {
        self.proxy_status_ident = Some(ident.clone());
    }

    pub(crate) fn set_optional_proxy_status_ident(&mut self, ident: Option<&HttpServerId>) {
        if let Some(id) = ident {
            self.set_proxy_status_ident(id);
        }
    }

    pub(crate) fn apply_proxy_status(
        &mut self,
        no_proxy_status: bool,
        ident: Option<&HttpServerId>,
    ) {
        if no_proxy_status {
            self.omit_proxy_status = true;
        } else {
            self.set_optional_proxy_status_ident(ident);
        }
    }

    fn append_proxy_status_header(&self, header: &mut Vec<u8>) {
        if self.omit_proxy_status {
            return;
        }
        let Some(error) = self.proxy_error else {
            return;
        };
        let ident = self
            .proxy_status_ident
            .as_ref()
            .map(HttpServerId::as_str)
            .unwrap_or(http_header::DEFAULT_PROXY_STATUS_IDENT);
        header.extend_from_slice(http_header::proxy_status(ident, error).as_bytes());
    }

    pub(crate) fn add_extra_header(&mut self, line: String) {
        self.extra_headers.push(line);
    }

    pub(crate) fn set_upstream_addr(&mut self, addr: SocketAddr) {
        self.extra_headers.push(http_header::upstream_addr(addr));
    }

    pub(crate) fn set_outgoing_ip(&mut self, ip: IpAddr) {
        self.extra_headers.push(http_header::outgoing_ip(ip));
    }

    #[inline]
    pub(crate) fn set_error_message(&mut self, msg: &'static str) {
        self.custom_error_message = Some(msg);
    }

    #[inline]
    pub(crate) fn too_many_requests(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::TOO_MANY_REQUESTS, version, true)
            .with_proxy_error(ProxyErrorType::HttpRequestError)
    }

    #[inline]
    pub(crate) fn forbidden(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::FORBIDDEN, version, true)
            .with_proxy_error(ProxyErrorType::HttpRequestDenied)
    }

    #[inline]
    pub(crate) fn method_not_allowed(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::METHOD_NOT_ALLOWED, version, true)
            .with_proxy_error(ProxyErrorType::HttpRequestError)
    }

    #[allow(unused)]
    #[inline]
    pub(crate) fn unimplemented(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::NOT_IMPLEMENTED, version, true)
            .with_proxy_error(ProxyErrorType::HttpRequestError)
    }

    #[inline]
    pub(crate) fn bad_request(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::BAD_REQUEST, version, true)
            .with_proxy_error(ProxyErrorType::HttpRequestError)
    }

    #[inline]
    pub(crate) fn bad_gateway(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
            .with_proxy_error(ProxyErrorType::HttpProtocolError)
    }

    #[inline]
    pub(crate) fn service_unavailable(version: Version) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::SERVICE_UNAVAILABLE, version, true)
            .with_proxy_error(ProxyErrorType::DestinationUnavailable)
    }

    #[inline]
    pub(crate) fn resource_not_found(version: Version, close: bool) -> Self {
        HttpProxyClientResponse::from_standard(StatusCode::NOT_FOUND, version, close)
            .with_proxy_error(ProxyErrorType::HttpRequestError)
    }

    pub(crate) fn need_login(version: Version, close: bool, realm: &str) -> Self {
        let mut response =
            HttpProxyClientResponse::from_standard(StatusCode::UNAUTHORIZED, version, close)
                .with_proxy_error(ProxyErrorType::HttpRequestDenied);
        let auth_header = vey_http::header::www_authenticate_basic(realm);
        response.add_extra_header(auth_header);
        response
    }

    pub(crate) fn proxy_auth_required(version: Version, close: bool, realm: &str) -> Self {
        let mut response = HttpProxyClientResponse::from_standard(
            StatusCode::PROXY_AUTHENTICATION_REQUIRED,
            version,
            close,
        )
        .with_proxy_error(ProxyErrorType::HttpRequestDenied);
        let auth_header = vey_http::header::proxy_authenticate_basic(realm);
        response.add_extra_header(auth_header);
        response
    }

    pub(crate) fn auto_chunked_ok(
        version: Version,
        close: bool,
        content_type: &Mime,
    ) -> (Self, bool) {
        let mut response = HttpProxyClientResponse::from_standard(StatusCode::OK, version, close);
        let chunked = if close {
            false
        } else if matches!(version, Version::HTTP_09 | Version::HTTP_10) {
            response.close = true;
            false
        } else {
            response.add_extra_header(vey_http::header::transfer_encoding_chunked().to_owned());
            true
        };
        response.add_extra_header(vey_http::header::content_type(content_type));
        (response, chunked)
    }

    pub(crate) fn sized_ok(
        version: Version,
        close: bool,
        body_len: u64,
        content_type: &Mime,
    ) -> Self {
        let mut response = HttpProxyClientResponse::from_standard(StatusCode::OK, version, close);
        response.add_extra_header(vey_http::header::content_length(body_len));
        response.add_extra_header(vey_http::header::content_type(content_type));
        response
    }

    pub(crate) fn ending_ok(version: Version, close: bool, content_type: &Mime) -> Self {
        let mut response = HttpProxyClientResponse::from_standard(StatusCode::OK, version, close);
        response.add_extra_header(vey_http::header::content_type(content_type));
        response
    }

    pub(crate) fn ok(version: Version, close: bool) -> Self {
        let mut response = HttpProxyClientResponse::from_standard(StatusCode::OK, version, close);
        response.add_extra_header(vey_http::header::content_length(0));
        response
    }

    pub(crate) fn sized_partial_content(
        version: Version,
        close: bool,
        start_size: u64,
        end_size: u64,
        total_size: u64,
        content_type: &Mime,
    ) -> Self {
        let mut response =
            HttpProxyClientResponse::from_standard(StatusCode::PARTIAL_CONTENT, version, close);
        response.add_extra_header(vey_http::header::content_range_sized(
            start_size, end_size, total_size,
        ));
        response.add_extra_header(vey_http::header::content_length(end_size - start_size + 1));
        response.add_extra_header(vey_http::header::content_type(content_type));
        response
    }

    pub(crate) fn range_not_satisfiable(
        version: Version,
        close: bool,
        start_size: Option<u64>,
    ) -> Self {
        let mut response = HttpProxyClientResponse::from_standard(
            StatusCode::RANGE_NOT_SATISFIABLE,
            version,
            close,
        )
        .with_proxy_error(ProxyErrorType::HttpRequestError);
        if let Some(start) = start_size {
            response.add_extra_header(vey_http::header::content_range_overflowed(start));
        }
        response
    }

    pub(crate) fn from_request_error(e: &HttpRequestParseError, version: Version) -> Option<Self> {
        e.status_code().map(|status| {
            HttpProxyClientResponse::from_standard(status, version, true)
                .with_proxy_error(proxy_error_for_client_status(status))
        })
    }

    pub(crate) fn from_ftp_connect_error(
        e: &FtpConnectError<TcpConnectError>,
        version: Version,
        should_close: bool,
    ) -> Self {
        match e {
            FtpConnectError::ConnectIoError(e) => {
                HttpProxyClientResponse::from_tcp_connect_error(e, version, should_close)
            }
            FtpConnectError::ConnectTimedOut => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, true)
                    .with_proxy_error(ProxyErrorType::ConnectionTimeout)
            }
            FtpConnectError::GreetingTimedOut => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, true)
                    .with_proxy_error(ProxyErrorType::HttpResponseTimeout)
            }
            FtpConnectError::GreetingFailed(_)
            | FtpConnectError::NegotiationFailed(_)
            | FtpConnectError::InvalidReplyCode(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::HttpProtocolError)
            }
            FtpConnectError::ServiceNotAvailable => HttpProxyClientResponse::from_standard(
                StatusCode::SERVICE_UNAVAILABLE,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::DestinationUnavailable),
        }
    }

    pub(crate) fn from_tcp_connect_error(
        e: &TcpConnectError,
        version: Version,
        should_close: bool,
    ) -> Self {
        let close = should_close;
        match e {
            TcpConnectError::MethodUnavailable => HttpProxyClientResponse::forbidden(version),
            TcpConnectError::EscaperNotUsable(_) => HttpProxyClientResponse::from_standard(
                StatusCode::SERVICE_UNAVAILABLE,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::DestinationUnavailable),
            TcpConnectError::ResolveFailed(e) => HttpProxyClientResponse::from_standard(
                StatusCode::from_u16(CustomStatusCode::ORIGIN_DNS_ERROR).unwrap(),
                version,
                close,
            )
            .with_proxy_error(proxy_error_for_resolve(e)),
            TcpConnectError::SetupSocketFailed(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            TcpConnectError::ConnectFailed(e) => {
                HttpProxyClientResponse::from_net_connect_err(e, version, should_close)
            }
            TcpConnectError::TimeoutByRule => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionTimeout)
            }
            TcpConnectError::NoAddressConnected => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionRefused)
            }
            TcpConnectError::ForbiddenAddressFamily | TcpConnectError::ForbiddenRemoteAddress => {
                HttpProxyClientResponse::from_standard(StatusCode::FORBIDDEN, version, close)
                    .with_proxy_error(ProxyErrorType::DestinationIpProhibited)
            }
            TcpConnectError::ProxyProtocolEncodeError(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            TcpConnectError::ProxyProtocolWriteFailed(_)
            | TcpConnectError::NegotiationReadFailed(_)
            | TcpConnectError::NegotiationWriteFailed(_)
            | TcpConnectError::NegotiationRejected(_)
            | TcpConnectError::NegotiationProtocolErr => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::HttpProtocolError)
            }
            TcpConnectError::NegotiationPeerTimeout
            | TcpConnectError::PeerTlsHandshakeTimeout
            | TcpConnectError::UpstreamTlsHandshakeTimeout => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionTimeout)
            }
            TcpConnectError::InternalServerError(_)
            | TcpConnectError::InternalTlsClientError(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            TcpConnectError::PeerTlsHandshakeFailed(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::TlsProtocolError),
            TcpConnectError::UpstreamTlsHandshakeFailed(_) => {
                HttpProxyClientResponse::from_standard(
                    StatusCode::from_u16(CustomStatusCode::SSL_HANDSHAKE_FAILED).unwrap(),
                    version,
                    close,
                )
                .with_proxy_error(ProxyErrorType::TlsProtocolError)
            }
        }
    }

    pub(crate) fn from_udp_connect_error(
        e: &UdpConnectError,
        version: Version,
        should_close: bool,
    ) -> Self {
        let close = should_close;
        match e {
            UdpConnectError::MethodUnavailable => HttpProxyClientResponse::forbidden(version),
            UdpConnectError::EscaperNotUsable(_) => HttpProxyClientResponse::from_standard(
                StatusCode::SERVICE_UNAVAILABLE,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::DestinationUnavailable),
            UdpConnectError::ResolveFailed(e) => HttpProxyClientResponse::from_standard(
                StatusCode::from_u16(CustomStatusCode::ORIGIN_DNS_ERROR).unwrap(),
                version,
                close,
            )
            .with_proxy_error(proxy_error_for_resolve(e)),
            UdpConnectError::SetupSocketFailed(_)
            | UdpConnectError::ProxyProtocolEncodeError(_)
            | UdpConnectError::InternalServerError(_)
            | UdpConnectError::InternalTlsClientError(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            UdpConnectError::ForbiddenAddressFamily | UdpConnectError::ForbiddenRemoteAddress => {
                HttpProxyClientResponse::forbidden(version)
            }
            UdpConnectError::NegotiationPeerTimeout
            | UdpConnectError::UnderlyingTimeoutByRule
            | UdpConnectError::PeerTlsHandshakeTimeout => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionTimeout)
            }
            UdpConnectError::UnderlyingTcpConnectFailed(e) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(proxy_error_for_connect(e))
            }
            UdpConnectError::ProxyProtocolWriteFailed(_)
            | UdpConnectError::NegotiationReadFailed(_)
            | UdpConnectError::NegotiationWriteFailed(_)
            | UdpConnectError::NegotiationRejected(_)
            | UdpConnectError::NegotiationProtocolErr
            | UdpConnectError::UnderlyingNoAddressConnected => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::HttpProtocolError)
            }
            UdpConnectError::PeerTlsHandshakeFailed(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::TlsProtocolError)
            }
        }
    }

    pub(crate) fn from_task_err(
        e: &ServerTaskError,
        version: Version,
        should_close: bool,
    ) -> Option<Self> {
        let close = should_close; // no retry on the same connection if there's body pending
        let r = match e {
            ServerTaskError::InternalServerError(_)
            | ServerTaskError::InternalAdapterError(_)
            | ServerTaskError::InternalResolverError(_)
            | ServerTaskError::UnclassifiedError(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                close,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            ServerTaskError::InternalTlsClientError(_) => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            ServerTaskError::PeerTlsHandshakeTimeout => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionTimeout)
            }
            ServerTaskError::PeerTlsHandshakeFailed(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::TlsProtocolError)
            }
            ServerTaskError::EscaperNotUsable(_) => HttpProxyClientResponse::from_standard(
                StatusCode::SERVICE_UNAVAILABLE,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::DestinationUnavailable),
            ServerTaskError::ForbiddenByRule(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::FORBIDDEN, version, true)
                    .with_proxy_error(ProxyErrorType::HttpRequestDenied)
            }
            ServerTaskError::InvalidClientProtocol(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_REQUEST, version, true)
                    .with_proxy_error(ProxyErrorType::HttpRequestError)
            }
            ServerTaskError::ClientAppError(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_REQUEST, version, true)
                    .with_proxy_error(ProxyErrorType::HttpRequestError)
            }
            ServerTaskError::UnimplementedProtocol => {
                HttpProxyClientResponse::from_standard(StatusCode::NOT_IMPLEMENTED, version, true)
                    .with_proxy_error(ProxyErrorType::HttpRequestError)
            }
            ServerTaskError::ClientAuthFailed => {
                // not in this stage
                return None;
            }
            ServerTaskError::UpstreamNotResolved(e) => HttpProxyClientResponse::from_standard(
                StatusCode::from_u16(CustomStatusCode::ORIGIN_DNS_ERROR).unwrap(),
                version,
                close,
            )
            .with_proxy_error(proxy_error_for_resolve(e)),
            ServerTaskError::UpstreamNotConnected(e) => {
                Self::from_net_connect_err(e, version, should_close)
            }
            ServerTaskError::UpstreamNotAvailable => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionRefused)
            }
            ServerTaskError::InvalidUpstreamProtocol(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::HttpProtocolError)
            }
            ServerTaskError::UpstreamReadFailed(_)
            | ServerTaskError::UpstreamWriteFailed(_)
            | ServerTaskError::ClosedByUpstream => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::ConnectionTerminated)
            }
            ServerTaskError::UpstreamNotNegotiated(_) | ServerTaskError::UpstreamAppError(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, true)
                    .with_proxy_error(ProxyErrorType::HttpProtocolError)
            }
            ServerTaskError::UpstreamTlsHandshakeTimeout => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, close)
                    .with_proxy_error(ProxyErrorType::ConnectionTimeout)
            }
            ServerTaskError::UpstreamTlsHandshakeFailed(_) => {
                HttpProxyClientResponse::from_standard(
                    StatusCode::from_u16(CustomStatusCode::SSL_HANDSHAKE_FAILED).unwrap(),
                    version,
                    close,
                )
                .with_proxy_error(ProxyErrorType::TlsProtocolError)
            }
            ServerTaskError::UpstreamAppUnavailable => HttpProxyClientResponse::from_standard(
                StatusCode::SERVICE_UNAVAILABLE,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::DestinationUnavailable),
            ServerTaskError::UpstreamAppTimeout(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::GATEWAY_TIMEOUT, version, true)
                    .with_proxy_error(ProxyErrorType::HttpResponseTimeout)
            }
            ServerTaskError::ClientAppTimeout(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::REQUEST_TIMEOUT, version, true)
                    .with_proxy_error(ProxyErrorType::HttpRequestError)
            }
            ServerTaskError::CanceledAsUserBlocked => {
                HttpProxyClientResponse::from_standard(StatusCode::FORBIDDEN, version, true)
                    .with_proxy_error(ProxyErrorType::HttpRequestDenied)
            }
            ServerTaskError::CanceledAsServerQuit => HttpProxyClientResponse::from_standard(
                StatusCode::INTERNAL_SERVER_ERROR,
                version,
                true,
            )
            .with_proxy_error(ProxyErrorType::ProxyInternalError),
            ServerTaskError::ClientTcpReadFailed(_)
            | ServerTaskError::ClientTcpWriteFailed(_)
            | ServerTaskError::ClientUdpRecvFailed(_)
            | ServerTaskError::ClientUdpSendFailed(_)
            | ServerTaskError::ClosedByClient
            | ServerTaskError::ClosedEarlyByClient
            | ServerTaskError::Idle(_, _)
            | ServerTaskError::InterceptionError(_, _)
            | ServerTaskError::Finished => return None,
        };
        Some(r)
    }

    fn from_net_connect_err(e: &ConnectError, version: Version, should_close: bool) -> Self {
        let close = should_close;
        match e {
            ConnectError::ConnectionRefused => HttpProxyClientResponse::from_standard(
                StatusCode::from_u16(CustomStatusCode::WEB_SERVER_IS_DOWN).unwrap(),
                version,
                close,
            )
            .with_proxy_error(ProxyErrorType::ConnectionRefused),
            ConnectError::ConnectionReset => HttpProxyClientResponse::from_standard(
                StatusCode::from_u16(CustomStatusCode::WEB_SERVER_IS_DOWN).unwrap(),
                version,
                close,
            )
            .with_proxy_error(ProxyErrorType::ConnectionTerminated),
            ConnectError::NetworkUnreachable | ConnectError::HostUnreachable => {
                HttpProxyClientResponse::from_standard(
                    StatusCode::from_u16(CustomStatusCode::ORIGIN_IS_UNREACHABLE).unwrap(),
                    version,
                    close,
                )
                .with_proxy_error(ProxyErrorType::DestinationIpUnroutable)
            }
            ConnectError::TimedOut => HttpProxyClientResponse::from_standard(
                StatusCode::from_u16(CustomStatusCode::CONNECTION_TIMED_OUT).unwrap(),
                version,
                close,
            )
            .with_proxy_error(ProxyErrorType::ConnectionTimeout),
            ConnectError::UnspecifiedError(_) => {
                HttpProxyClientResponse::from_standard(StatusCode::BAD_GATEWAY, version, close)
                    .with_proxy_error(ProxyErrorType::ProxyInternalError)
            }
        }
    }

    pub(crate) fn should_close(&self) -> bool {
        self.close
    }

    fn canonical_reason(&self) -> &'static str {
        let code = self.status.as_u16();
        self.status
            .canonical_reason()
            .unwrap_or_else(|| CustomStatusCode::canonical_reason(code))
    }

    pub(crate) async fn reply_ok_to_connect<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut header = Vec::<u8>::with_capacity(Self::RESPONSE_BUFFER_SIZE);
        write!(
            header,
            "{:?} {} {}\r\n",
            self.version,
            self.status.as_str(),
            self.canonical_reason(),
        )?;
        for line in &self.extra_headers {
            header.extend_from_slice(line.as_bytes());
        }
        header.extend_from_slice(b"\r\n");
        writer.write_all_flush(header.as_ref()).await?;
        Ok(())
    }

    pub(crate) async fn reply_ok_header<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut header = Vec::<u8>::with_capacity(Self::RESPONSE_BUFFER_SIZE);
        write!(
            header,
            "{:?} {} {}\r\n",
            self.version,
            self.status.as_str(),
            self.canonical_reason(),
        )?;
        for line in &self.extra_headers {
            header.extend_from_slice(line.as_bytes());
        }
        header.extend_from_slice(vey_http::header::connection_as_bytes(self.close));
        header.extend_from_slice(b"\r\n");
        writer.write_all(header.as_ref()).await?;
        // writer.flush().await?;
        Ok(())
    }

    pub(crate) async fn reply_continue<W>(version: Version, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let s = format!("{version:?} 100 Continue\r\n\r\n");
        writer.write_all_flush(s.as_bytes()).await?;
        Ok(())
    }

    async fn reply_err<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        let code = self.status.as_str();
        let reason = self.canonical_reason();
        let body = if let Some(msg) = &self.custom_error_message {
            format!(
                "<html>\n\
                 <head><title>{code} {reason}</title></head>\n\
                 <body>\n\
                 <div style=\"text-align: center;\"><h1>{msg}</h1></div>\n\
                 </body>\n\
                 </html>\n",
            )
        } else {
            format!(
                "<html>\n\
                 <head><title>{code} {reason}</title></head>\n\
                 <body>\n\
                 <div style=\"text-align: center;\"><h1>{code} {reason}</h1></div>\n\
                 </body>\n\
                 </html>\n"
            )
        };

        let mut header = Vec::<u8>::with_capacity(Self::RESPONSE_BUFFER_SIZE);
        write!(
            header,
            "{:?} {} {reason}\r\n",
            self.version,
            self.status.as_str(),
        )?;
        for line in &self.extra_headers {
            header.extend_from_slice(line.as_bytes());
        }
        self.append_proxy_status_header(&mut header);
        header.extend_from_slice(vey_http::header::content_type(&mime::TEXT_HTML).as_bytes());
        header.extend_from_slice(vey_http::header::content_length(body.len() as u64).as_bytes());
        header.extend_from_slice(vey_http::header::connection_as_bytes(self.close));
        header.extend_from_slice(b"\r\n");
        // append body
        header.extend_from_slice(body.as_bytes());

        writer.write_all_flush(header.as_ref()).await?;
        Ok(())
    }

    pub(crate) async fn reply_err_to_request<W>(&self, writer: &mut W) -> io::Result<()>
    where
        W: AsyncWrite + Unpin,
    {
        self.reply_err(writer).await
    }
}

fn proxy_error_for_resolve(e: &ResolveError) -> ProxyErrorType {
    match e {
        ResolveError::RequestTimeout | ResolveError::DriverTimeout => ProxyErrorType::DnsTimeout,
        _ => ProxyErrorType::DnsError,
    }
}

fn proxy_error_for_connect(e: &ConnectError) -> ProxyErrorType {
    match e {
        ConnectError::ConnectionRefused => ProxyErrorType::ConnectionRefused,
        ConnectError::ConnectionReset => ProxyErrorType::ConnectionTerminated,
        ConnectError::NetworkUnreachable | ConnectError::HostUnreachable => {
            ProxyErrorType::DestinationIpUnroutable
        }
        ConnectError::TimedOut => ProxyErrorType::ConnectionTimeout,
        ConnectError::UnspecifiedError(_) => ProxyErrorType::ProxyInternalError,
    }
}

fn proxy_error_for_client_status(status: StatusCode) -> ProxyErrorType {
    match status {
        StatusCode::FORBIDDEN => ProxyErrorType::HttpRequestDenied,
        StatusCode::LOOP_DETECTED => ProxyErrorType::ProxyLoopDetected,
        _ => ProxyErrorType::HttpRequestError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Version;

    fn status_and_error(rsp: &HttpProxyClientResponse) -> (u16, Option<&'static str>) {
        (rsp.status(), rsp.proxy_error.map(ProxyErrorType::as_token))
    }

    fn proxy_status_line(rsp: &HttpProxyClientResponse) -> Option<String> {
        if rsp.omit_proxy_status {
            return None;
        }
        rsp.proxy_error.map(|error| {
            http_header::proxy_status(
                rsp.proxy_status_ident
                    .as_ref()
                    .map(HttpServerId::as_str)
                    .unwrap_or(http_header::DEFAULT_PROXY_STATUS_IDENT),
                error,
            )
        })
    }

    #[test]
    fn ok_response_has_no_proxy_status() {
        let rsp = HttpProxyClientResponse::ok(Version::HTTP_11, false);
        assert_eq!(status_and_error(&rsp), (200, None));
        assert!(proxy_status_line(&rsp).is_none());
    }

    #[test]
    fn connect_timeout_maps_to_proxy_status() {
        let rsp = HttpProxyClientResponse::from_tcp_connect_error(
            &TcpConnectError::TimeoutByRule,
            Version::HTTP_11,
            false,
        );
        assert_eq!(rsp.status(), 504);
        assert_eq!(
            proxy_status_line(&rsp).as_deref(),
            Some("Proxy-Status: vey-proxy; error=connection_timeout\r\n")
        );
    }

    #[test]
    fn dns_and_tls_use_custom_codes_with_proxy_status() {
        let dns = HttpProxyClientResponse::from_tcp_connect_error(
            &TcpConnectError::ResolveFailed(ResolveError::EmptyResult),
            Version::HTTP_11,
            false,
        );
        assert_eq!(status_and_error(&dns), (530, Some("dns_error")));

        let dns_timeout = HttpProxyClientResponse::from_tcp_connect_error(
            &TcpConnectError::ResolveFailed(ResolveError::RequestTimeout),
            Version::HTTP_11,
            false,
        );
        assert_eq!(status_and_error(&dns_timeout), (530, Some("dns_timeout")));

        let tls = HttpProxyClientResponse::from_tcp_connect_error(
            &TcpConnectError::UpstreamTlsHandshakeFailed(anyhow::anyhow!("tls")),
            Version::HTTP_11,
            false,
        );
        assert_eq!(status_and_error(&tls), (525, Some("tls_protocol_error")));
    }

    #[test]
    fn net_connect_errors_keep_custom_codes() {
        let refused = HttpProxyClientResponse::from_net_connect_err(
            &ConnectError::ConnectionRefused,
            Version::HTTP_11,
            false,
        );
        assert_eq!(
            status_and_error(&refused),
            (521, Some("connection_refused"))
        );

        let unreachable = HttpProxyClientResponse::from_net_connect_err(
            &ConnectError::HostUnreachable,
            Version::HTTP_11,
            false,
        );
        assert_eq!(
            status_and_error(&unreachable),
            (523, Some("destination_ip_unroutable"))
        );

        let timed_out = HttpProxyClientResponse::from_net_connect_err(
            &ConnectError::TimedOut,
            Version::HTTP_11,
            false,
        );
        assert_eq!(
            status_and_error(&timed_out),
            (522, Some("connection_timeout"))
        );
    }

    #[test]
    fn server_id_replaces_default_ident() {
        let ident: HttpServerId = "edge-1".parse().unwrap();
        let mut rsp = HttpProxyClientResponse::forbidden(Version::HTTP_11);
        rsp.set_proxy_status_ident(&ident);
        assert_eq!(
            proxy_status_line(&rsp).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_denied\r\n")
        );

        let mut rate_limited = HttpProxyClientResponse::too_many_requests(Version::HTTP_11);
        rate_limited.set_optional_proxy_status_ident(Some(&ident));
        assert_eq!(
            proxy_status_line(&rate_limited).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_error\r\n")
        );

        let mut method = HttpProxyClientResponse::method_not_allowed(Version::HTTP_11);
        method.set_optional_proxy_status_ident(Some(&ident));
        assert_eq!(
            proxy_status_line(&method).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_error\r\n")
        );

        let mut bad_request = HttpProxyClientResponse::bad_request(Version::HTTP_11);
        bad_request.set_optional_proxy_status_ident(Some(&ident));
        assert_eq!(
            proxy_status_line(&bad_request).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_error\r\n")
        );

        let mut unimplemented = HttpProxyClientResponse::unimplemented(Version::HTTP_11);
        unimplemented.set_optional_proxy_status_ident(Some(&ident));
        assert_eq!(
            proxy_status_line(&unimplemented).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_error\r\n")
        );

        let mut proxy_auth =
            HttpProxyClientResponse::proxy_auth_required(Version::HTTP_11, true, "proxy");
        proxy_auth.set_optional_proxy_status_ident(Some(&ident));
        assert_eq!(
            proxy_status_line(&proxy_auth).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_denied\r\n")
        );

        let mut origin_auth = HttpProxyClientResponse::need_login(Version::HTTP_11, true, "ftp");
        origin_auth.set_optional_proxy_status_ident(Some(&ident));
        assert_eq!(
            proxy_status_line(&origin_auth).as_deref(),
            Some("Proxy-Status: edge-1; error=http_request_denied\r\n")
        );
    }

    #[test]
    fn proxy_status_can_be_disabled() {
        let ident: HttpServerId = "edge-1".parse().unwrap();
        let mut rsp = HttpProxyClientResponse::forbidden(Version::HTTP_11);
        rsp.apply_proxy_status(true, Some(&ident));
        assert!(proxy_status_line(&rsp).is_none());
    }

    #[test]
    fn loop_detected_parse_error() {
        let rsp = HttpProxyClientResponse::from_request_error(
            &HttpRequestParseError::LoopDetected,
            Version::HTTP_11,
        )
        .unwrap();
        assert_eq!(status_and_error(&rsp), (508, Some("proxy_loop_detected")));
    }

    #[test]
    fn local_forbidden_and_auth_errors() {
        let rsp = HttpProxyClientResponse::from_task_err(
            &ServerTaskError::ForbiddenByRule(crate::serve::ServerTaskForbiddenError::DestDenied),
            Version::HTTP_11,
            true,
        )
        .unwrap();
        assert_eq!(status_and_error(&rsp), (403, Some("http_request_denied")));
    }
}
