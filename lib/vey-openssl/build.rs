/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

use std::env;

#[allow(clippy::unusual_byte_groupings)]
fn main() {
    println!("cargo:rustc-check-cfg=cfg(libressl)");
    println!("cargo:rustc-check-cfg=cfg(tongsuo)");
    println!("cargo:rustc-check-cfg=cfg(boringssl)");
    println!("cargo:rustc-check-cfg=cfg(awslc)");
    println!("cargo:rustc-check-cfg=cfg(awslc_fips)");

    println!("cargo:rustc-check-cfg=cfg(ossl300)");

    if env::var("DEP_OPENSSL_LIBRESSL").is_ok() {
        println!("cargo:rustc-cfg=libressl");
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=LibreSSL");
    } else if env::var("DEP_OPENSSL_TONGSUO").is_ok() {
        println!("cargo:rustc-cfg=tongsuo");
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=Tongsuo");
    } else if env::var("DEP_OPENSSL_BORINGSSL").is_ok() {
        println!("cargo:rustc-cfg=boringssl");
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=BoringSSL");
    } else if env::var("DEP_OPENSSL_AWSLC_FIPS").is_ok() {
        println!("cargo:rustc-cfg=awslc");
        println!("cargo:rustc-cfg=awslc_fips");
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=AWS-LC-FIPS");
    } else if env::var("DEP_OPENSSL_AWSLC").is_ok() {
        println!("cargo:rustc-cfg=awslc");
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=AWS-LC");
    } else {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=OpenSSL");
    }

    if let Ok(version) = env::var("DEP_OPENSSL_VERSION_NUMBER") {
        // this will require a dependency on openssl-sys crate
        let version = u64::from_str_radix(&version, 16).unwrap();

        if version >= 0x3_00_00_00_0 {
            println!("cargo:rustc-cfg=ossl300");
        }
    }
}
