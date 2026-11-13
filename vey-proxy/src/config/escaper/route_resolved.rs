/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ip_network::IpNetwork;
use saphyr::{Scalar, Yaml};

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_types::resolve::ResolveStrategy;
use vey_yaml::YamlDocPosition;

use super::{AnyEscaperConfig, EscaperConfig, EscaperConfigDiffAction, EscaperConfigVerifier};

const ESCAPER_CONFIG_TYPE: &str = "RouteResolved";

#[derive(Clone, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = RouteResolved,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct RouteResolvedEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) resolver: NodeName,
    pub(crate) resolve_strategy: ResolveStrategy,
    pub(crate) resolution_delay: Duration,
    pub(crate) lpm_rules: BTreeMap<NodeName, BTreeSet<IpNetwork>>,
    pub(crate) default_next: NodeName,
}

impl RouteResolvedEscaperConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        RouteResolvedEscaperConfig {
            name: NodeName::default(),
            position,
            resolver: NodeName::default(),
            resolve_strategy: Default::default(),
            resolution_delay: Duration::from_millis(50),
            lpm_rules: BTreeMap::new(),
            default_next: NodeName::default(),
        }
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut config = Self::new(position);

        vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;

        config.check()?;
        Ok(config)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_ESCAPER_TYPE => Ok(()),
            super::CONFIG_KEY_ESCAPER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "resolver" => {
                self.resolver = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "resolve_strategy" => {
                self.resolve_strategy = vey_yaml::value::as_resolve_strategy(v)?;
                Ok(())
            }
            "resolution_delay" => {
                self.resolution_delay = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "lpm_match" | "lpm_rules" => {
                if let Yaml::Sequence(seq) = v {
                    for (i, rule) in seq.iter().enumerate() {
                        if let Yaml::Mapping(map) = rule {
                            self.add_lpm_rule(map)?;
                        } else {
                            return Err(anyhow!("invalid value type for {k}#{i}"));
                        }
                    }
                    Ok(())
                } else {
                    Err(anyhow!("invalid array value for key {k}"))
                }
            }
            "default_next" => {
                self.default_next = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.resolver.is_empty() {
            return Err(anyhow!("no resolver is set"));
        }
        if self.default_next.is_empty() {
            return Err(anyhow!("no default next escaper is set"));
        }
        if !self.lpm_rules.is_empty() {
            EscaperConfigVerifier::check_duplicated_rule(&self.lpm_rules)
                .context("found duplicated network")?;
        }
        Ok(())
    }

    fn add_lpm_rule(&mut self, map: &saphyr::Mapping) -> anyhow::Result<()> {
        let mut escaper = NodeName::default();
        let mut networks = BTreeSet::<IpNetwork>::new();
        vey_yaml::foreach_kv(map, |k, v| match vey_yaml::key::normalize(k).as_str() {
            "next" | "escaper" => {
                escaper = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "nets" | "net" | "networks" | "network" => {
                if let Yaml::Sequence(seq) = v {
                    for (i, obj) in seq.iter().enumerate() {
                        if let Yaml::Value(Scalar::String(net)) = obj {
                            let net = IpNetwork::from_str(net).map_err(|e| {
                                anyhow!("invalid network string for {k}#{i}: {e:?}")
                            })?;
                            networks.insert(net);
                        } else {
                            return Err(anyhow!("invalid network string value for {k}#{i}"));
                        }
                    }
                    Ok(())
                } else {
                    Err(anyhow!("invalid array value for key {k}"))
                }
            }
            _ => Err(anyhow!("invalid key {k}")),
        })?;
        if escaper.is_empty() {
            return Err(anyhow!("no next escaper set"));
        }
        if let Some(_old) = self.lpm_rules.insert(escaper.clone(), networks) {
            return Err(anyhow!("found multiple entries for next escaper {escaper}"));
        }
        Ok(())
    }
}

impl EscaperConfig for RouteResolvedEscaperConfig {
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
        &self.resolver
    }

    fn dependent_escaper(&self) -> Option<BTreeSet<NodeName>> {
        let mut set = BTreeSet::new();
        set.insert(self.default_next.clone());
        for key in self.lpm_rules.keys() {
            set.insert(key.clone());
        }
        Some(set)
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::RouteResolvedEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfigDiffAction};

    fn parse(doc: &Yaml) -> RouteResolvedEscaperConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        RouteResolvedEscaperConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            resolver: main
            default_next: next
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::RouteResolved(new)),
            EscaperConfigDiffAction::NoAction
        ));
    }
}
