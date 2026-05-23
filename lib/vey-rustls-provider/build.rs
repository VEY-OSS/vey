/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::env;

fn main() {
    if env::var("CARGO_FEATURE_RUSTLS_RING").is_ok() {
        println!("cargo:rustc-env=VEY_RUSTLS_PROVIDER=ring");
    }
    if env::var("CARGO_FEATURE_RUSTLS_AWS_LC").is_ok() {
        println!("cargo:rustc-env=VEY_RUSTLS_PROVIDER=aws-lc");
    }
    if env::var("CARGO_FEATURE_RUSTLS_AWS_LC_FIPS").is_ok() {
        println!("cargo:rustc-env=VEY_RUSTLS_PROVIDER=aws-lc-fips");
    }
}
