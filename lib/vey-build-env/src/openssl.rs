/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::env;

pub fn check_openssl() {
    if env::var("DEP_OPENSSL_LIBRESSL").is_ok() {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=LibreSSL");
    } else if env::var("DEP_OPENSSL_TONGSUO").is_ok() {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=Tongsuo");
    } else if env::var("DEP_OPENSSL_BORINGSSL").is_ok() {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=BoringSSL");
    } else if env::var("DEP_OPENSSL_AWSLC_FIPS").is_ok() {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=AWS-LC-FIPS");
    } else if env::var("DEP_OPENSSL_AWSLC").is_ok() {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=AWS-LC");
    } else {
        println!("cargo:rustc-env=VEY_OPENSSL_VARIANT=OpenSSL");
    }
}
