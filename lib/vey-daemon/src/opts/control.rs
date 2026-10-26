/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::anyhow;

static CONTROL_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn validate_and_set_control_dir(app_name: &str, path: Option<&Path>) -> anyhow::Result<()> {
    let default_dir;
    let path = match path {
        Some(path) => {
            if path.is_relative() {
                return Err(anyhow!("{} is not an absolute path", path.display()));
            }
            path
        }
        None => {
            default_dir = vey_compat::default_control_dir(app_name);
            &default_dir
        }
    };

    if path.exists() && !path.is_dir() {
        return Err(anyhow!("{} is existed but not a directory", path.display()));
    }

    CONTROL_DIR
        .set(path.to_path_buf())
        .map_err(|_| anyhow!("control directory has already been set"))
}

pub fn control_dir() -> PathBuf {
    CONTROL_DIR
        .get()
        .cloned()
        .expect("control directory has not been set")
}
