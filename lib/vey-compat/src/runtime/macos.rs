/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;

pub(super) fn runtime_base_dir() -> PathBuf {
    let uid = rustix::process::getuid().as_raw();
    if uid == 0 {
        return PathBuf::from("/var/run");
    }
    // macOS has no `/run/user/<uid>`. The Darwin per-user temporary directory
    // is private and local. An absolute `XDG_RUNTIME_DIR` still wins.
    if let Some(dir) = super::absolute_xdg_runtime_dir() {
        return dir;
    }
    std::env::temp_dir()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::runtime_base_dir;

    #[test]
    fn runtime_base_dir_follows_the_user() {
        let uid = rustix::process::getuid().as_raw();
        let dir = runtime_base_dir();
        if uid == 0 {
            assert_eq!(dir, Path::new("/var/run"));
            return;
        }
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(xdg) if Path::new(&xdg).is_absolute() => assert_eq!(dir, PathBuf::from(xdg)),
            _ => assert_eq!(dir, std::env::temp_dir()),
        }
    }
}
