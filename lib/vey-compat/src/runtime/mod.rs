/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::runtime_base_dir;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::runtime_base_dir;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod other;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use other::runtime_base_dir;

/// Directory that holds control sockets when `--control-dir` is omitted.
///
/// `app_name` is the last component, such as `vey-proxy`, so each program has
/// its own directory.
pub fn default_control_dir(app_name: &str) -> PathBuf {
    let mut dir = runtime_base_dir();
    dir.push(app_name);
    dir
}

fn absolute_xdg_runtime_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
    dir.is_absolute().then_some(dir)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::default_control_dir;

    #[test]
    fn default_control_dir_uses_the_app_name() {
        let dir = default_control_dir("vey-proxy");
        assert_eq!(dir.file_name(), Some(OsStr::new("vey-proxy")));
    }
}
