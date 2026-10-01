/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

// Certificate decompression references `brotli` only on AWS-LC, BoringSSL, and Tongsuo.
// On other OpenSSL backends this keeps the dependency used for `cargo::unused_dependencies`.
#[cfg(not(any(awslc, boringssl, tongsuo)))]
use brotli as _;

mod client;
pub use client::{
    OpensslClientConfig, OpensslClientConfigBuilder, OpensslInterceptionClientConfig,
    OpensslInterceptionClientConfigBuilder,
};

mod server;
pub use server::{
    OpensslInterceptionServerConfig, OpensslInterceptionServerConfigBuilder, OpensslServerConfig,
    OpensslServerConfigBuilder, OpensslServerSessionCache, OpensslSessionIdContext,
    OpensslTicketKey, OpensslTicketKeyBuilder,
};

mod cert_pair;
pub use cert_pair::OpensslCertificatePair;

mod tlcp_cert_pair;
pub use tlcp_cert_pair::OpensslTlcpCertificatePair;

mod protocol;
pub use protocol::OpensslProtocol;
