/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::Path;

use anyhow::anyhow;
use yaml_rust::{Yaml, yaml};

pub(crate) mod geoip;

pub(crate) mod server;

pub fn load(program_name: &str) -> anyhow::Result<&'static Path> {
    let config_file =
        vey_daemon::opts::config_file().ok_or_else(|| anyhow!("no config file set"))?;

    // allow multiple docs, and treat them as the same
    vey_yaml::foreach_doc(config_file, |_, doc| match doc {
        Yaml::Hash(map) => load_doc(map, program_name),
        _ => Err(anyhow!("yaml doc root should be hash")),
    })?;

    if server::is_empty() {
        return Err(anyhow!("no server configured"));
    }
    crate::geoip::load();

    Ok(config_file)
}

pub(crate) async fn reload() -> anyhow::Result<()> {
    tokio::task::spawn_blocking(reload_blocking)
        .await
        .map_err(|e| anyhow!("failed to join reload task: {e}"))?
}

fn reload_blocking() -> anyhow::Result<()> {
    server::clear();
    if let Some(config_file) = vey_daemon::opts::config_file() {
        vey_yaml::foreach_doc(config_file, |_, doc| match doc {
            Yaml::Hash(map) => reload_doc(map),
            _ => Err(anyhow!("yaml doc root should be hash")),
        })?;
    }
    Ok(())
}

fn reload_doc(map: &yaml::Hash) -> anyhow::Result<()> {
    let conf_dir = vey_daemon::opts::config_dir()
        .ok_or_else(|| anyhow!("no valid config dir has been set"))?;
    vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
        "runtime" | "worker" | "stat" | "controller" => Ok(()),
        "server" => server::load_all(v),
        "geoip_db" => geoip::load(v, conf_dir),
        _ => Ok(()),
    })?;
    Ok(())
}

fn load_doc(map: &yaml::Hash, program_name: &str) -> anyhow::Result<()> {
    let conf_dir = vey_daemon::opts::config_dir()
        .ok_or_else(|| anyhow!("no valid config dir has been set"))?;
    vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
        "runtime" => vey_daemon::runtime::config::load(v),
        "worker" => vey_daemon::runtime::config::load_worker(v),
        "stat" => vey_daemon::stat::config::load(v, program_name),
        "controller" => vey_daemon::control::config::load(v),
        "server" => server::load_all(v),
        "geoip_db" => geoip::load(v, conf_dir),
        _ => Err(anyhow!("invalid key {k} in main conf")),
    })?;
    Ok(())
}
