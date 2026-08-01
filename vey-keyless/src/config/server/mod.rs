/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_macros::AnyConfig;
use vey_types::metrics::NodeName;
use vey_yaml::{HybridParser, YamlDocPosition};

mod registry;
pub(crate) use registry::{clear, get_all};

pub(crate) mod cloudflare;
pub(crate) mod plain_tcp_port;
pub(crate) mod plain_tls_port;

const CONFIG_KEY_SERVER_TYPE: &str = "type";
const CONFIG_KEY_SERVER_NAME: &str = "name";

pub(crate) enum KeyServerConfigDiffAction {
    NoAction,
    SpawnNew,
    ReloadAndRespawn,
}

pub(crate) trait KeyServerConfig {
    fn name(&self) -> &NodeName;
    fn position(&self) -> Option<YamlDocPosition>;
    fn r#type(&self) -> &'static str;

    fn diff_action(&self, new: &AnyKeyServerConfig) -> KeyServerConfigDiffAction;

    /// The names of the servers this one will send accepted connections to.
    fn dependent_server(&self) -> Option<NodeName> {
        None
    }
}

#[derive(Clone, AnyConfig)]
#[def_fn(name, &NodeName)]
#[def_fn(position, Option<YamlDocPosition>)]
#[def_fn(r#type, &'static str)]
#[def_fn(dependent_server, Option<NodeName>)]
#[def_fn(diff_action, &Self, KeyServerConfigDiffAction)]
pub(crate) enum AnyKeyServerConfig {
    Cloudflare(cloudflare::CloudflareServerConfig),
    PlainTcpPort(plain_tcp_port::PlainTcpPortConfig),
    PlainTlsPort(plain_tls_port::PlainTlsPortConfig),
}

pub(crate) fn load_all(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(v, |map, position| {
        let server = load_server(map, position)?;
        registry::add(server, false)?;
        Ok(())
    })?;
    check_dependency()
}

/// Make sure every `server` reference points to another server that really exists.
fn check_dependency() -> anyhow::Result<()> {
    let all_config = get_all();
    let all_names = all_config
        .iter()
        .map(|config| config.name().clone())
        .collect::<HashSet<_>>();

    for config in &all_config {
        let Some(dependent) = config.dependent_server() else {
            continue;
        };
        let name = config.name();
        if dependent.eq(name) {
            return Err(anyhow!(
                "server {name}{} can not use itself as its next server",
                describe_position(config)
            ));
        }
        if !all_names.contains(&dependent) {
            return Err(anyhow!(
                "no server {dependent} found, which is required by server {name}{}",
                describe_position(config)
            ));
        }
    }
    Ok(())
}

fn describe_position(config: &AnyKeyServerConfig) -> String {
    match config.position() {
        Some(position) => format!(" (at {position})"),
        None => String::new(),
    }
}

fn load_server(
    map: &yaml::Hash,
    position: Option<YamlDocPosition>,
) -> anyhow::Result<AnyKeyServerConfig> {
    // the server type is optional, and defaults to the cloudflare keyless server
    let server_type = vey_yaml::hash_get_optional_str(map, CONFIG_KEY_SERVER_TYPE)?
        .unwrap_or(cloudflare::SERVER_CONFIG_TYPE);
    match vey_yaml::key::normalize(server_type).as_str() {
        "cloudflare" | "cloudflare_keyless" | "cloudflarekeyless" => {
            let server = cloudflare::CloudflareServerConfig::parse(map, position)
                .context("failed to load this Cloudflare server")?;
            Ok(AnyKeyServerConfig::Cloudflare(server))
        }
        "plain_tcp_port" | "plaintcpport" | "plain_tcp" | "plaintcp" => {
            let server = plain_tcp_port::PlainTcpPortConfig::parse(map, position)
                .context("failed to load this PlainTcpPort server")?;
            Ok(AnyKeyServerConfig::PlainTcpPort(server))
        }
        "plain_tls_port" | "plaintlsport" | "plain_tls" | "plaintls" => {
            let server = plain_tls_port::PlainTlsPortConfig::parse(map, position)
                .context("failed to load this PlainTlsPort server")?;
            Ok(AnyKeyServerConfig::PlainTlsPort(server))
        }
        _ => Err(anyhow!("unsupported server type {server_type}")),
    }
}
