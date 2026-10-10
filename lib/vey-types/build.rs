/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 */

use std::env;

#[allow(clippy::unusual_byte_groupings)]
fn gen_openssl_flags() {
    println!("cargo:rustc-check-cfg=cfg(libressl)");
    println!("cargo:rustc-check-cfg=cfg(tongsuo)");
    println!("cargo:rustc-check-cfg=cfg(tongsuo850)");
    println!("cargo:rustc-check-cfg=cfg(boringssl)");
    println!("cargo:rustc-check-cfg=cfg(awslc)");

    if env::var("DEP_OPENSSL_LIBRESSL").is_ok() {
        println!("cargo:rustc-cfg=libressl");
    }

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

    if env::var("DEP_OPENSSL_BORINGSSL").is_ok() {
        println!("cargo:rustc-cfg=boringssl");
    }

    if env::var("DEP_OPENSSL_AWSLC").is_ok() {
        println!("cargo:rustc-cfg=awslc");
    }
}

fn main() {
    gen_openssl_flags();
}
