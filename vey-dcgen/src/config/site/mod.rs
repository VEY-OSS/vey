/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::Yaml;

use vey_types::metrics::NodeName;
use vey_yaml::HybridParser;

mod config;
pub(crate) use config::{SiteConfig, SiteKeyCert};

mod registry;
pub(crate) use registry::{clear, get_all};

pub(super) fn load_one(name: &str) -> anyhow::Result<SiteConfig> {
    let id = name
        .parse::<NodeName>()
        .map_err(|e| anyhow!("invalid site id {name}: {e}"))?;
    let Some(old) = registry::get(&id) else {
        return Err(anyhow!("no site {id}"));
    };
    let Some(position) = old.position().cloned() else {
        return Err(anyhow!(
            "no config position for site {id}, reload is not supported"
        ));
    };
    let config = SiteConfig::load_at_position(&position)?;
    if config.id() != &id {
        return Err(anyhow!(
            "site at position {position} has id {}, while we expect {id}",
            config.id()
        ));
    }
    let _ = registry::add(config.clone());
    Ok(config)
}

pub(super) fn load_all(value: &Yaml) -> anyhow::Result<()> {
    let conf_dir = vey_daemon::opts::config_dir()
        .ok_or_else(|| anyhow!("no valid config dir has been set"))?;
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(value, |map, position| {
        let config = SiteConfig::parse(map, position)?;
        if let Some(old) = registry::add(config) {
            Err(anyhow!("site {} already exists", old.id()))
        } else {
            Ok(())
        }
    })
}
