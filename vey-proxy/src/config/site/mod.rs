/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::path::Path;
use std::sync::Arc;

use anyhow::anyhow;
use yaml_rust::{Yaml, yaml};

use vey_daemon::config::TopoMap;
use vey_yaml::{HybridParser, YamlDocPosition};

mod group;
pub(crate) use group::SiteGroupConfig;

mod registry;
pub(crate) use registry::clear;

mod config;
pub(crate) use config::SiteConfig;

mod http;
pub(crate) use http::SiteHttpConfig;

pub(crate) fn load_all(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(v, |map, position| {
        let group = load_site_group(map, position)?;
        if let Some(old_group) = registry::add(group) {
            Err(anyhow!(
                "site group with name {} already exists",
                old_group.name()
            ))
        } else {
            Ok(())
        }
    })?;
    build_topology_map()?;
    Ok(())
}

pub(crate) fn load_at_position(position: &YamlDocPosition) -> anyhow::Result<SiteGroupConfig> {
    let doc = vey_yaml::load_doc(position)?;
    if let Yaml::Hash(map) = doc {
        let group = load_site_group(&map, Some(position.clone()))?;
        let old_group = registry::add(group.clone());
        if let Err(e) = build_topology_map() {
            // rollback
            match old_group {
                Some(group) => {
                    registry::add(group);
                }
                None => registry::del(group.name()),
            }
            Err(e)
        } else {
            Ok(group)
        }
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

fn build_topology_map() -> anyhow::Result<TopoMap> {
    let mut topo_map = TopoMap::default();

    for name in registry::get_all_names() {
        topo_map.add_node(&name, &|name| {
            let conf = registry::get(name)?;
            conf.dependent_site_group()
        })?;
    }

    Ok(topo_map)
}

pub(crate) fn get_all_sorted() -> anyhow::Result<Vec<Arc<SiteGroupConfig>>> {
    let topo_map = build_topology_map()?;
    let sorted_nodes = topo_map.sorted_nodes();
    let mut sorted_conf = Vec::with_capacity(sorted_nodes.len());
    for node in sorted_nodes {
        let Some(conf) = registry::get(node.name()) else {
            continue;
        };
        sorted_conf.push(conf);
    }
    Ok(sorted_conf)
}
