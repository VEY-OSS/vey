/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

#[cfg(not(any(
    feature = "rustls-ring",
    feature = "rustls-aws-lc",
    feature = "rustls-aws-lc-fips"
)))]
compile_error!("quic crypto requires rustls-ring, rustls-aws-lc, or rustls-aws-lc-fips");

#[cfg(any(
    feature = "rustls-ring",
    feature = "rustls-aws-lc",
    feature = "rustls-aws-lc-fips"
))]
mod provider;

#[cfg(any(
    feature = "rustls-ring",
    feature = "rustls-aws-lc",
    feature = "rustls-aws-lc-fips"
))]
pub use provider::{HandshakeTokenKey, ResetKey};
