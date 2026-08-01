/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

fn main() {
    let lib = pkg_config::Config::new()
        .probe("crypto-mb")
        .expect("crypto-mb not found; install Intel crypto_mb and pkg-config metadata");

    for path in &lib.include_paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
