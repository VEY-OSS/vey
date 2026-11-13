/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_yaml::YamlDocPosition;

use super::{EscaperConfig, EscaperConfigDiffAction};
use crate::config::escaper::AnyEscaperConfig;

const ESCAPER_CONFIG_DEFAULT_TYPE: &str = "DummyDeny";

#[derive(Clone, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = DummyDeny,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
)]
pub(crate) struct DummyDenyEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    custom_type: String,
    pub(crate) extra_metrics_tags: Option<Arc<MetricTagMap>>,
}

impl DummyDenyEscaperConfig {
    pub(crate) fn new(position: Option<YamlDocPosition>, custom_type: Option<&str>) -> Self {
        DummyDenyEscaperConfig {
            name: NodeName::default(),
            position,
            custom_type: match custom_type {
                Some(custom_type) => custom_type.to_owned(),
                None => ESCAPER_CONFIG_DEFAULT_TYPE.to_owned(),
            },
            extra_metrics_tags: None,
        }
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
        custom_type: Option<&str>,
    ) -> anyhow::Result<Self> {
        let mut escaper = Self::new(position, custom_type);
        vey_yaml::foreach_kv(map, |k, v| escaper.set(k, v))?;
        escaper.check()?;
        Ok(escaper)
    }

    fn check(&self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
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
            "extra_metrics_tags" => {
                let tags = vey_yaml::value::as_static_metrics_tags(v)
                    .context(format!("invalid static metrics tags value for key {k}"))?;
                self.extra_metrics_tags = Some(Arc::new(tags));
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

impl EscaperConfig for DummyDenyEscaperConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn r#type(&self) -> &str {
        &self.custom_type
    }

    fn resolver(&self) -> &NodeName {
        Default::default()
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::DummyDenyEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfigDiffAction};

    fn parse(doc: &Yaml) -> DummyDenyEscaperConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        DummyDenyEscaperConfig::parse(map, None, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::DummyDeny(new)),
            EscaperConfigDiffAction::NoAction
        ));
    }
}
