/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2023-2025 ByteDance and/or its affiliates.
 */

use std::path::Path;

use anyhow::anyhow;
use yaml_rust::{Yaml, yaml};

mod backend;
pub(crate) use backend::{OpensslBackendConfig, get_config as get_backend_config};

pub fn load() -> anyhow::Result<&'static Path> {
    let config_file =
        vey_daemon::opts::config_file().ok_or_else(|| anyhow!("no config file set"))?;

    // allow multiple docs, and treat them as the same
    vey_yaml::foreach_doc(config_file, |_, doc| match doc {
        Yaml::Hash(map) => load_doc(map),
        _ => Err(anyhow!("yaml doc root should be hash")),
    })?;

    Ok(config_file)
}

fn load_doc(map: &yaml::Hash) -> anyhow::Result<()> {
    vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
        "runtime" => vey_daemon::runtime::config::load(v),
        "worker" => vey_daemon::runtime::config::load_worker(v),
        "stat" => vey_daemon::stat::config::load(v, crate::build::PKG_NAME),
        "backend" => backend::load_config(v),
        _ => Err(anyhow!("invalid key {k} in main conf")),
    })?;
    Ok(())
}
