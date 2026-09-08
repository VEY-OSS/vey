/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use openssl::ssl::Ssl;

use vey_types::net::{AlpnProtocol, Host, OpensslClientConfig, UpstreamAddr};

use super::TcpConnectError;

pub(crate) struct TcpConnectTaskConf<'a> {
    pub(crate) upstream: &'a UpstreamAddr,
}

pub(crate) struct TlsConnectTaskConf<'a> {
    pub(crate) tcp: TcpConnectTaskConf<'a>,
    pub(crate) tls_config: &'a OpensslClientConfig,
    pub(crate) tls_name: &'a Host,
    /// ALPN offered on this SSL, not on the shared client SslCtx.
    pub(crate) alpn_protocols: Option<&'static [AlpnProtocol]>,
}

impl TlsConnectTaskConf<'_> {
    pub(crate) fn build_ssl(&self) -> Result<Ssl, TcpConnectError> {
        let port = self.tcp.upstream.port();
        let ssl = match self.alpn_protocols {
            Some(alpn) => self
                .tls_config
                .build_ssl_with_alpn(self.tls_name, port, alpn),
            None => self.tls_config.build_ssl(self.tls_name, port),
        };
        ssl.map_err(TcpConnectError::InternalTlsClientError)
    }

    pub(crate) fn handshake_timeout(&self) -> Duration {
        self.tls_config.handshake_timeout
    }
}
