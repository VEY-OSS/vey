/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use cfg_if::cfg_if;

pub fn install_default() -> anyhow::Result<()> {
    // TODO use cfg_select

    cfg_if! {
        if #[cfg(any(feature = "rustls-aws-lc", feature = "rustls-aws-lc-fips"))] {
            rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .map_err(|e| anyhow!("failed to install aws-lc provider: {e:?}"))
        } else if #[cfg(feature = "rustls-ring")] {
            rustls::crypto::ring::default_provider()
                .install_default()
                .map_err(|e| anyhow!("failed to install ring provider: {e:?}"))
        } else {
            compile_error!("no rustls provider can be used")
        }
    }
}
