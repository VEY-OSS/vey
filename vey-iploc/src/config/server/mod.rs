/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_macros::AnyConfig;
use vey_types::metrics::NodeName;
use vey_yaml::HybridParser;

mod registry;
pub(crate) use registry::{clear, get_all, is_empty};

mod dummy;
pub(crate) use dummy::DummyServerConfig;

mod udp;
pub(crate) use udp::UdpServerConfig;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use unix::UnixServerConfig;

const CONFIG_KEY_SERVER_TYPE: &str = "type";

pub(crate) enum ServerConfigDiffAction {
    NoAction,
    SpawnNew,
    ReloadAndRespawn,
}

pub(crate) trait ServerConfig {
    fn name(&self) -> &NodeName;
    fn diff_action(&self, new: &AnyServerConfig) -> ServerConfigDiffAction;
}

#[derive(Clone, Debug, Eq, PartialEq, AnyConfig)]
#[def_fn(name, &NodeName)]
#[def_fn(diff_action, &Self, ServerConfigDiffAction)]
pub(crate) enum AnyServerConfig {
    Dummy(DummyServerConfig),
    Udp(UdpServerConfig),
    #[cfg(unix)]
    Unix(UnixServerConfig),
}

pub(crate) fn load_all(v: &Yaml) -> anyhow::Result<()> {
    let conf_dir = vey_daemon::opts::config_dir()
        .ok_or_else(|| anyhow!("no valid config dir has been set"))?;
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(v, |map, _position| {
        let server = load_server(map)?;
        if let Some(old) = registry::add(server) {
            Err(anyhow!("server with name {} already exists", old.name()))
        } else {
            Ok(())
        }
    })?;
    Ok(())
}

fn load_server(map: &yaml::Hash) -> anyhow::Result<AnyServerConfig> {
    let server_type = vey_yaml::hash_get_required_str(map, CONFIG_KEY_SERVER_TYPE)?;
    match vey_yaml::key::normalize(server_type).as_str() {
        "dummy" => {
            let server =
                DummyServerConfig::parse(map).context("failed to load this Dummy server")?;
            Ok(AnyServerConfig::Dummy(server))
        }
        "udp" => {
            let server = UdpServerConfig::parse(map).context("failed to load this UDP server")?;
            Ok(AnyServerConfig::Udp(server))
        }
        #[cfg(unix)]
        "unix" => {
            let server = UnixServerConfig::parse(map).context("failed to load this Unix server")?;
            Ok(AnyServerConfig::Unix(server))
        }
        #[cfg(not(unix))]
        "unix" => Err(anyhow!("unix server is not supported on this platform")),
        _ => Err(anyhow!("unsupported server type {server_type}")),
    }
}
