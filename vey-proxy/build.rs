/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::env;

#[allow(clippy::unusual_byte_groupings)]
fn main() {
    vey_build_env::check_basic();

    println!("cargo:rustc-check-cfg=cfg(tongsuo)");
    println!("cargo:rustc-check-cfg=cfg(tongsuo850)");
    if env::var("DEP_OPENSSL_TONGSUO").is_ok() {
        println!("cargo:rustc-cfg=tongsuo");

        if let Ok(version) = env::var("DEP_OPENSSL_TONGSUO_VERSION_NUMBER") {
            // this will require a dependency on openssl-sys crate
            let version = u64::from_str_radix(&version, 16).unwrap();

            if version >= 0x8_05_00_00_0 {
                println!("cargo:rustc-cfg=tongsuo850");
            }
        }
    }

    if env::var("CARGO_FEATURE_LUA").is_ok() {
        if env::var("CARGO_FEATURE_LUA54").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=lua54");
        } else if env::var("CARGO_FEATURE_LUA55").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=lua55");
        } else if env::var("CARGO_FEATURE_LUAJIT").is_ok() {
            println!("cargo:rustc-env=VEY_LUA_FEATURE=luajit");
        }
    }

    if env::var("CARGO_FEATURE_QUIC").is_ok() {
        println!("cargo:rustc-env=VEY_QUIC_FEATURE=quinn");
    }
}
