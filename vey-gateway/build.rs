/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use std::env;

fn main() {
    vey_build_env::check_basic();
    vey_build_env::check_openssl();
    vey_build_env::check_rustls_provider();

    if env::var("CARGO_FEATURE_QUIC").is_ok() {
        println!("cargo:rustc-env=VEY_QUIC_FEATURE=quinn");
    }
}
