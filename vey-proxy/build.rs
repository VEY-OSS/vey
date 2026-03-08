/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use std::env;

fn main() {
    vey_build_env::check_basic();
    vey_build_env::check_openssl();
    vey_build_env::check_rustls_provider();

    if env::var("CARGO_FEATURE_LUA").is_ok() {
        if env::var("CARGO_FEATURE_LUA53").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=lua53");
        } else if env::var("CARGO_FEATURE_LUA54").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=lua54");
        } else if env::var("CARGO_FEATURE_LUA55").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=lua55");
        } else if env::var("CARGO_FEATURE_LUAJIT").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=luajit");
        }
    }

    if env::var("CARGO_FEATURE_PYTHON").is_ok() {
        println!("cargo:rustc-env=VEY_PYTHON_FEATURE=python");
    }

    if env::var("CARGO_FEATURE_C_ARES").is_ok() {
        println!("cargo:rustc-env=VEY_C_ARES_FEATURE=c-ares");
    }

    if env::var("CARGO_FEATURE_QUIC").is_ok() {
        println!("cargo:rustc-env=VEY_QUIC_FEATURE=quinn");
    }
}
