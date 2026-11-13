/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::borrow::Cow;
use std::path::Path;

use anyhow::{Context, anyhow};
use saphyr::{Mapping, Scalar, Yaml};

use vey_macros::AnyConfig;
use vey_types::metrics::NodeName;
use vey_yaml::{HybridParser, YamlDocPosition};

mod registry;
pub(crate) use registry::{clear, get_all};

pub(crate) mod host_resolver;
pub(crate) mod static_addr;

const CONFIG_KEY_DISCOVER_TYPE: &str = "type";
const CONFIG_KEY_DISCOVER_NAME: &str = "name";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiscoverRegisterData {
    Null,
    Yaml(Yaml<'static>),
    #[allow(unused)]
    Json(serde_json::Value),
}

impl DiscoverRegisterData {
    pub(crate) fn from_yaml(value: &Yaml<'_>) -> Self {
        Self::Yaml(yaml_to_static(value))
    }
}

fn yaml_to_static(value: &Yaml<'_>) -> Yaml<'static> {
    match value {
        Yaml::Value(Scalar::Null) => Yaml::Value(Scalar::Null),
        Yaml::Value(Scalar::Boolean(b)) => Yaml::Value(Scalar::Boolean(*b)),
        Yaml::Value(Scalar::Integer(i)) => Yaml::Value(Scalar::Integer(*i)),
        Yaml::Value(Scalar::FloatingPoint(n)) => Yaml::Value(Scalar::FloatingPoint(*n)),
        Yaml::Value(Scalar::String(s)) => Yaml::Value(Scalar::String(Cow::Owned(s.to_string()))),
        Yaml::Sequence(seq) => Yaml::Sequence(seq.iter().map(yaml_to_static).collect()),
        Yaml::Mapping(map) => {
            let mut out = Mapping::new();
            for (k, v) in map {
                out.insert(yaml_to_static(k), yaml_to_static(v));
            }
            Yaml::Mapping(out)
        }
        Yaml::Tagged(tag, node) => Yaml::Tagged(
            Cow::Owned(tag.clone().into_owned()),
            Box::new(yaml_to_static(node)),
        ),
        Yaml::Representation(repr, style, tag) => Yaml::Representation(
            Cow::Owned(repr.to_string()),
            *style,
            tag.as_ref().map(|t| Cow::Owned(t.clone().into_owned())),
        ),
        Yaml::Alias(id) => Yaml::Alias(*id),
        Yaml::BadValue => Yaml::BadValue,
    }
}

pub(crate) enum DiscoverConfigDiffAction {
    NoAction,
    SpawnNew,
    #[allow(unused)]
    UpdateInPlace,
}

pub(crate) trait DiscoverConfig {
    fn name(&self) -> &NodeName;
    fn position(&self) -> Option<YamlDocPosition>;
    fn r#type(&self) -> &'static str;
}

#[derive(Clone, AnyConfig)]
#[def_fn(name, &NodeName)]
#[def_fn(r#type, &'static str)]
#[def_fn(position, Option<YamlDocPosition>)]
#[def_fn(diff_action, &Self, DiscoverConfigDiffAction)]
pub(crate) enum AnyDiscoverConfig {
    StaticAddr(static_addr::StaticAddrDiscoverConfig),
    HostResolver(host_resolver::HostResolverDiscoverConfig),
}

pub(crate) fn load_all(v: &Yaml, conf_dir: &Path) -> anyhow::Result<()> {
    let parser = HybridParser::new(conf_dir, vey_daemon::opts::config_file_extension());
    parser.foreach_map(v, |map, position| {
        let site = load_discover(map, position)?;
        registry::add(site, false)?;
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn load_at_position(position: &YamlDocPosition) -> anyhow::Result<AnyDiscoverConfig> {
    let doc = vey_yaml::load_doc(position)?;
    if let Yaml::Mapping(map) = doc {
        let site = load_discover(&map, Some(position.clone()))?;
        registry::add(site.clone(), true)?;
        Ok(site)
    } else {
        Err(anyhow!("yaml doc {position} is not a map"))
    }
}

fn load_discover(
    map: &saphyr::Mapping,
    position: Option<YamlDocPosition>,
) -> anyhow::Result<AnyDiscoverConfig> {
    let discover_type = vey_yaml::hash_get_required_str(map, CONFIG_KEY_DISCOVER_TYPE)?;
    match vey_yaml::key::normalize(discover_type).as_str() {
        "static_addr" | "staticaddr" => {
            let discover = static_addr::StaticAddrDiscoverConfig::parse_yaml_conf(map, position)
                .context("failed to load this StaticAddr discover")?;
            Ok(AnyDiscoverConfig::StaticAddr(discover))
        }
        "host_resolver" | "hostresolver" => {
            let discover =
                host_resolver::HostResolverDiscoverConfig::parse_yaml_conf(map, position)
                    .context("failed to load this HostResolver discover")?;
            Ok(AnyDiscoverConfig::HostResolver(discover))
        }
        _ => Err(anyhow!("unsupported discover type {}", discover_type)),
    }
}
