/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

fn main() {
    if let Ok(lib) = pkg_config::Config::new().probe("crypto-mb") {
        for path in &lib.include_paths {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    } else {
        // fallback to use the system installed libcrypto_mb
        println!("cargo:rustc-link-lib=crypto_mb");
    }
}
