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
    // An absolute `XDG_RUNTIME_DIR` is the session directory when a login
    // manager created one. Otherwise the XDG state home can be created by the
    // user. Without `HOME`, the last resort is `{temp}/vey-<uid>`.
    if let Some(dir) = super::absolute_xdg_runtime_dir() {
        return dir;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let mut dir = PathBuf::from(home);
        dir.push(".local");
        dir.push("state");
        return dir;
    }
    let mut dir = std::env::temp_dir();
    dir.push(format!("vey-{uid}"));
    dir
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
            _ => {
                if let Some(home) = std::env::var_os("HOME") {
                    let mut expected = PathBuf::from(home);
                    expected.push(".local");
                    expected.push("state");
                    assert_eq!(dir, expected);
                } else {
                    let mut expected = std::env::temp_dir();
                    expected.push(format!("vey-{uid}"));
                    assert_eq!(dir, expected);
                }
            }
        }
    }
}
