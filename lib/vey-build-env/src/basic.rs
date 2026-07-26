/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::env;

pub fn check_basic() {
    let rustc = rustc_version::version_meta().unwrap();
    println!(
        "cargo:rustc-env=VEY_BUILD_RUSTC_VERSION={}",
        rustc.short_version_string
    );
    println!(
        "cargo:rustc-env=VEY_BUILD_RUSTC_CHANNEL={:?}",
        rustc.channel
    );

    println!(
        "cargo:rustc-env=VEY_BUILD_HOST={}",
        env::var("HOST").unwrap()
    );
    println!(
        "cargo:rustc-env=VEY_BUILD_TARGET={}",
        env::var("TARGET").unwrap()
    );
    println!(
        "cargo:rustc-env=VEY_BUILD_PROFILE={}",
        env::var("PROFILE").unwrap()
    );
    println!(
        "cargo:rustc-env=VEY_BUILD_OPT_LEVEL={}",
        env::var("OPT_LEVEL").unwrap()
    );
    println!(
        "cargo:rustc-env=VEY_BUILD_DEBUG={}",
        env::var("DEBUG").unwrap()
    );

    if let Ok(v) = env::var("VEY_PACKAGE_VERSION") {
        println!("cargo:rustc-env=VEY_PACKAGE_VERSION={v}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustc_version_meta_is_populated() {
        let meta = rustc_version::version_meta().unwrap();
        assert!(!meta.short_version_string.is_empty());
    }

    #[test]
    fn check_basic_requires_cargo_build_env() {
        // check_basic() reads HOST/TARGET/PROFILE/etc. set by Cargo during build scripts.
        if std::env::var("HOST").is_err() {
            return;
        }
        check_basic();
    }
}
