/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 */

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

const RUSTC_VERSION: &str = env!("VEY_BUILD_RUSTC_VERSION");
const RUSTC_CHANNEL: &str = env!("VEY_BUILD_RUSTC_CHANNEL");

const BUILD_HOST: &str = env!("VEY_BUILD_HOST");
const BUILD_TARGET: &str = env!("VEY_BUILD_TARGET");
const BUILD_PROFILE: &str = env!("VEY_BUILD_PROFILE");
const BUILD_OPT_LEVEL: &str = env!("VEY_BUILD_OPT_LEVEL");
const BUILD_DEBUG: &str = env!("VEY_BUILD_DEBUG");

const PACKAGE_VERSION: Option<&str> = option_env!("VEY_PACKAGE_VERSION");

const QUIC_FEATURE: Option<&str> = option_env!("VEY_QUIC_FEATURE");

pub fn print_version() {
    println!("{PKG_NAME} {VERSION}");
    print!("Features:");
    #[cfg(feature = "jemalloc")]
    if let Some(version) = vey_jemalloc::lib_version() {
        print!(" jemalloc({})", version.to_string_lossy());
    }
    #[cfg(feature = "mimalloc")]
    println!(" mimalloc({})", vey_mimalloc::lib_version());
    if let Some(quic) = QUIC_FEATURE {
        print!(" {quic}");
    }
    println!();
    if let Some(variant) = vey_openssl::variant_name() {
        println!("OpenSSL Variant: {variant}");
    }
    if let Some(provider) = vey_rustls_provider::provider_name() {
        println!("Rustls Provider: {provider}");
    }
    println!("Compiler: {RUSTC_VERSION} ({RUSTC_CHANNEL})");
    println!("Host: {BUILD_HOST}, Target: {BUILD_TARGET}");
    println!("Profile: {BUILD_PROFILE}, Opt Level: {BUILD_OPT_LEVEL}, Debug: {BUILD_DEBUG}");
    if let Some(package_version) = PACKAGE_VERSION {
        println!("Package Version: {package_version}");
    }
}
