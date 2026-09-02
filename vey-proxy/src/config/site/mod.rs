/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::Path;

use anyhow::anyhow;
use yaml_rust::{Yaml, yaml};

use vey_yaml::{HybridParser, YamlDocPosition};

mod group;
pub(crate) use group::SiteGroupConfig;

mod registry;
pub(crate) use registry::{clear, get_all};

mod config;
pub(crate) use config::SiteConfig;

pub(crate) fn load_all(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(v, |map, position| {
        let group = load_site_group(map, position)?;
        registry::add(group, false)?;
        Ok(())
    })
}

pub(crate) fn load_at_position(position: &YamlDocPosition) -> anyhow::Result<SiteGroupConfig> {
    let doc = vey_yaml::load_doc(position)?;
    if let Yaml::Hash(map) = doc {
        let group = load_site_group(&map, Some(position.clone()))?;
        registry::add(group.clone(), true)?;
        Ok(group)
    } else {
        Err(anyhow!("yaml doc {position} is not a map"))
    }
}

fn load_site_group(
    map: &yaml::Hash,
    position: Option<YamlDocPosition>,
) -> anyhow::Result<SiteGroupConfig> {
    SiteGroupConfig::parse(map, position)
}
