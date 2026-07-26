/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use cfg_if::cfg_if;

const PROVIDER_NAME: Option<&str> = option_env!("VEY_RUSTLS_PROVIDER");

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

pub fn provider_name() -> Option<&'static str> {
    PROVIDER_NAME
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_default_succeeds() {
        install_default().expect("default rustls provider should install");
    }

    #[test]
    fn provider_name_matches_enabled_feature() {
        let name = provider_name().expect("build script should set provider name");
        cfg_if! {
            if #[cfg(feature = "rustls-aws-lc-fips")] {
                assert_eq!(name, "aws-lc-fips");
            } else if #[cfg(feature = "rustls-aws-lc")] {
                assert_eq!(name, "aws-lc");
            } else if #[cfg(feature = "rustls-ring")] {
                assert_eq!(name, "ring");
            } else {
                compile_error!("no rustls provider feature enabled for tests");
            }
        }
    }
}
