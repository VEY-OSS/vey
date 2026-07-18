/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use openssl::ssl::Ssl;

use vey_types::net::{Host, OpensslClientConfig, UpstreamAddr};

use super::TcpConnectError;

pub(crate) struct TcpConnectTaskConf<'a> {
    pub(crate) upstream: &'a UpstreamAddr,
}

pub(crate) struct TlsConnectTaskConf<'a> {
    pub(crate) tcp: TcpConnectTaskConf<'a>,
    pub(crate) tls_config: &'a OpensslClientConfig,
    pub(crate) tls_name: &'a Host,
}

impl TlsConnectTaskConf<'_> {
    pub(crate) fn build_ssl(&self) -> Result<Ssl, TcpConnectError> {
        self.tls_config
            .build_ssl(self.tls_name, self.tcp.upstream.port())
            .map_err(TcpConnectError::InternalTlsClientError)
    }

    pub(crate) fn handshake_timeout(&self) -> Duration {
        self.tls_config.handshake_timeout
    }
}
