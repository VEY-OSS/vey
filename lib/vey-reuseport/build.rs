/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};

fn main() {
    let mut common = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    common.push("src");
    common.push("bpf");
    common.push("common.h");
    println!("cargo:rerun-if-changed={}", common.display());

    compile_single("tcp");
    compile_single("udp");
    compile_single("quic");
}

fn compile_single(name: &str) {
    let mut source = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    source.push("src");
    source.push("bpf");
    source.push(format!("{name}.bpf.c"));

    println!("cargo:rerun-if-changed={}", source.display());

    let mut out = PathBuf::from(env::var("OUT_DIR").unwrap());
    out.push(format!("{name}.bpf.o"));

    let mut cmd = Command::new("clang");

    let mut arch_path = PathBuf::from("/usr/lib/linux/uapi");
    if let Ok(true) = fs::exists(&arch_path) {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        match target_arch.as_str() {
            "aarch64" => arch_path.push("arm64"),
            "loongarch64" => arch_path.push("loongarch"),
            "mips64" | "mips64el" | "mipsisa64r6" | "mipsisa64r6el" => arch_path.push("mips"),
            "powerpc64" | "powerpc64le" => arch_path.push("powerpc"),
            "riscv64a23" | "riscv64gc" => arch_path.push("riscv"),
            "s390x" => arch_path.push("s390"),
            "sparc64" => arch_path.push("sparc"),
            "x86_64" => arch_path.push("x86"),
            arch => {
                panic!("Unsupported architecture: {arch}");
            }
        }
        cmd.arg("-I").arg(&arch_path);
    } else {
        let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
        let dir = match target_arch.as_str() {
            "aarch64" => "aarch64-linux-gnu",
            "loongarch64" => "loongarch64-linux-gnu",
            "mips64" => "mips64-linux-gnuabi64",
            "mips64el" => "mips64el-linux-gnuabi64",
            "mipsisa64r6el" => "mipsisa64r6el-linux-gnuabi64",
            "powerpc64" => "powerpc64-linux-gnu",
            "powerpc64le" => "powerpc64le-linux-gnu",
            "riscv64a23" | "riscv64gc" => "riscv64-linux-gnu",
            "s390x" => "s390x-linux-gnu",
            "sparc64" => "sparc64-linux-gnu",
            "x86_64" => "x86_64-linux-gnu",
            arch => {
                panic!("Unsupported architecture: {arch}");
            }
        };
        let arch_path = format!("/usr/include/{dir}");
        if let Ok(true) = fs::exists(&arch_path) {
            cmd.arg("-I").arg(&arch_path);
        }
    }

    if let Ok(dir) = env::var("DEP_BPF_INCLUDE") {
        cmd.arg("-I").arg(&dir);
    }

    cmd.arg("-g")
        .arg("-O2")
        .arg("-target")
        .arg("bpf")
        .arg("-c")
        .arg(source)
        .arg("-o")
        .arg(out);

    let output = cmd.output().unwrap();
    if !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        for line in stderr.split("\n") {
            if !line.trim().is_empty() {
                println!("cargo:warning=STDERR:{}", line);
            }
        }
    }
    if !output.status.success() {
        panic!("{name} bpf compile failed");
    }
}
