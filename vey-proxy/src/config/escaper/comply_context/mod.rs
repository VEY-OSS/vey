/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright 2026 VEY-OSS Developers.
 */

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{EscaperConfig, EscaperConfigDiffAction};
use crate::config::escaper::AnyEscaperConfig;

mod egress_index;
mod egress_upstream;

use egress_index::EgressIndexConfig;
pub(crate) use egress_upstream::{EgressUpstream, EgressUpstreamConfig};

const ESCAPER_CONFIG_TYPE: &str = "ComplyContext";

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ComplyContextEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) next: NodeName,
    pub(crate) set_egress_upstream: Vec<EgressUpstreamConfig>,
    pub(crate) set_egress_index: Vec<EgressIndexConfig>,
}

impl ComplyContextEscaperConfig {
    pub(crate) fn new(position: Option<YamlDocPosition>) -> Self {
        ComplyContextEscaperConfig {
            name: NodeName::default(),
            position,
            next: NodeName::default(),
            set_egress_upstream: Vec::new(),
            set_egress_index: Vec::new(),
        }
    }

    pub(super) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut escaper = Self::new(position);
        vey_yaml::foreach_kv(map, |k, v| escaper.set(k, v))?;
        escaper.check()?;
        Ok(escaper)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.next.is_empty() {
            return Err(anyhow!("next escaper is not set"));
        }

        Ok(())
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match k {
            super::CONFIG_KEY_ESCAPER_TYPE => Ok(()),
            super::CONFIG_KEY_ESCAPER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "next" => {
                self.next = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "use_egress_upstream" => {
                self.set_egress_upstream = vey_yaml::value::as_list(v, EgressUpstreamConfig::parse)
                    .context(format!(
                        "invalid egress upstream config value list for key {k}"
                    ))?;
                Ok(())
            }
            "use_egress_index" => {
                self.set_egress_index = vey_yaml::value::as_list(v, EgressIndexConfig::parse)
                    .context(format!(
                        "invalid egress index config value list for key {k}"
                    ))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

impl EscaperConfig for ComplyContextEscaperConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &str {
        ESCAPER_CONFIG_TYPE
    }

    fn resolver(&self) -> &NodeName {
        Default::default()
    }

    fn diff_action(&self, new: &AnyEscaperConfig) -> EscaperConfigDiffAction {
        let AnyEscaperConfig::ComplyContext(new) = new else {
            return EscaperConfigDiffAction::SpawnNew;
        };

        if self.eq(new) {
            EscaperConfigDiffAction::NoAction
        } else {
            EscaperConfigDiffAction::Reload
        }
    }
}
