/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if os != "linux" || arch != "x86_64" {
        // Unsupported host/target: skip native linking so workspace builds on
        // macOS/Windows/etc. do not require qatlib.
        return;
    }

    // `qatlib.pc` Requires: libqat libusdm
    let lib = pkg_config::Config::new()
        .probe("qatlib")
        .expect("qatlib not found via pkg-config (install libqat-dev / libusdm-dev)");
    for path in &lib.include_paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
