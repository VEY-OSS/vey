/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;

use libbpf_cargo::SkeletonBuilder;

const UDP_BPF_SRC: &str = "src/bpf/udp.bpf.c";

fn main() {
    let mut out = PathBuf::from(env::var("OUT_DIR").unwrap());
    out.push("udp.skel.rs");

    SkeletonBuilder::new()
        .source(UDP_BPF_SRC)
        .clang_args([
            OsStr::new("-I"),
            OsStr::new("/usr/include/x86_64-linux-gnu/"),
        ])
        .build_and_generate(&out)
        .expect("failed to build and generate BPF skeleton for udp.bpf.c");

    println!("cargo:rerun-if-changed={UDP_BPF_SRC}");
}
