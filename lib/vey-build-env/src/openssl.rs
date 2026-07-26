/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::env;

/// Resolve the OpenSSL variant from build-time dependency flags.
pub fn openssl_variant() -> &'static str {
    if env::var("DEP_OPENSSL_LIBRESSL").is_ok() {
        "LibreSSL"
    } else if env::var("DEP_OPENSSL_TONGSUO").is_ok() {
        "Tongsuo"
    } else if env::var("DEP_OPENSSL_BORINGSSL").is_ok() {
        "BoringSSL"
    } else if env::var("DEP_OPENSSL_AWSLC_FIPS").is_ok() {
        "AWS-LC-FIPS"
    } else if env::var("DEP_OPENSSL_AWSLC").is_ok() {
        "AWS-LC"
    } else {
        "OpenSSL"
    }
}

pub fn check_openssl() {
    println!("cargo:rustc-env=VEY_OPENSSL_VARIANT={}", openssl_variant());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_openssl_smoke() {
        check_openssl();
    }

    #[test]
    fn openssl_variant_defaults_to_openssl_without_flags() {
        assert_eq!(openssl_variant(), "OpenSSL");
    }
}
