/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

mod ffi;

#[cfg(feature = "async-job")]
pub mod async_job;

mod ssl;
#[cfg(feature = "async-job")]
pub use ssl::SslAsyncModeExt;
#[cfg(not(libressl))]
pub use ssl::SslLazyAcceptor;
pub use ssl::{SslAcceptor, SslConnector, SslError, SslInfoCallbackWhere, SslStream};

const OPENSSL_VARIANT: Option<&str> = option_env!("VEY_OPENSSL_VARIANT");

pub fn variant_name() -> Option<&'static str> {
    OPENSSL_VARIANT
}
