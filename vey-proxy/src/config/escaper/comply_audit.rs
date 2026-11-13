/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2024-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use anyhow::anyhow;
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::NodeName;
use vey_yaml::YamlDocPosition;

use super::{EscaperConfig, EscaperConfigDiffAction};
use crate::config::escaper::AnyEscaperConfig;

const ESCAPER_CONFIG_TYPE: &str = "ComplyAudit";

#[derive(Clone, PartialEq, Eq, DiffAction)]
#[diff_action(
    new = AnyEscaperConfig,
    variant = ComplyAudit,
    mismatch = EscaperConfigDiffAction::SpawnNew,
    unchanged = EscaperConfigDiffAction::NoAction,
    changed = EscaperConfigDiffAction::Reload,
)]
pub(crate) struct ComplyAuditEscaperConfig {
    pub(crate) name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) next: NodeName,
    pub(crate) auditor: NodeName,
}

impl ComplyAuditEscaperConfig {
    pub(crate) fn new(position: Option<YamlDocPosition>) -> Self {
        ComplyAuditEscaperConfig {
            name: NodeName::default(),
            position,
            next: NodeName::default(),
            auditor: NodeName::default(),
        }
    }

    pub(super) fn parse(
        map: &saphyr::Mapping,
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
        if self.auditor.is_empty() {
            return Err(anyhow!("auditor is not set"));
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
            "auditor" => {
                self.auditor = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}

impl EscaperConfig for ComplyAuditEscaperConfig {
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
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::ComplyAuditEscaperConfig;
    use crate::config::escaper::{AnyEscaperConfig, EscaperConfigDiffAction};

    fn parse(doc: &Yaml) -> ComplyAuditEscaperConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        ComplyAuditEscaperConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            next: next
            auditor: audit
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyEscaperConfig::ComplyAudit(new)),
            EscaperConfigDiffAction::NoAction
        ));
    }
}
