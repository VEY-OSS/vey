/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::env;

fn main() {
    vey_build_env::check_basic();

    println!("cargo:rustc-check-cfg=cfg(tongsuo)");
    println!("cargo:rustc-check-cfg=cfg(libressl)");
    if env::var("DEP_OPENSSL_TONGSUO").is_ok() {
        println!("cargo:rustc-cfg=tongsuo");
    } else if env::var("DEP_OPENSSL_LIBRESSL").is_ok() {
        println!("cargo:rustc-cfg=libressl");
    }

    if env::var("CARGO_FEATURE_QUIC").is_ok() {
        println!("cargo:rustc-env=VEY_QUIC_FEATURE=quinn");
    }
}
