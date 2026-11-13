/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use saphyr::Yaml;

use vey_macros::DiffAction;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_yaml::YamlDocPosition;

use super::{AnyExporterConfig, ExporterConfig, ExporterConfigDiffAction};
use crate::runtime::export::StreamExportConfig;
use crate::types::MetricName;

const EXPORTER_CONFIG_TYPE: &str = "Graphite";

#[derive(Clone, Debug, Eq, PartialEq, DiffAction)]
#[diff_action(
    new = AnyExporterConfig,
    variant = Graphite,
    mismatch = ExporterConfigDiffAction::SpawnNew,
    unchanged = ExporterConfigDiffAction::NoAction,
    changed = ExporterConfigDiffAction::Reload,
)]
pub(crate) struct GraphiteExporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) emit_interval: Duration,
    pub(crate) stream_export: StreamExportConfig,
    pub(crate) prefix: Option<MetricName>,
    pub(crate) global_tags: MetricTagMap,
}

impl GraphiteExporterConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        GraphiteExporterConfig {
            name: NodeName::default(),
            position,
            emit_interval: Duration::from_secs(10),
            stream_export: StreamExportConfig::new(2003),
            prefix: None,
            global_tags: MetricTagMap::default(),
        }
    }

    pub(crate) fn parse(
        map: &saphyr::Mapping,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = GraphiteExporterConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| collector.set(k, v))?;

        collector.check()?;
        Ok(collector)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_EXPORTER_TYPE => Ok(()),
            super::CONFIG_KEY_EXPORTER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "emit_interval" => {
                self.emit_interval = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "prefix" => {
                let prefix = MetricName::parse_yaml(v)
                    .context(format!("invalid metric name value for key {k}"))?;
                self.prefix = Some(prefix);
                Ok(())
            }
            "global_tags" => {
                self.global_tags = vey_yaml::value::as_static_metrics_tags(v)
                    .context(format!("invalid static metrics tags value for key {k}"))?;
                Ok(())
            }
            _ => self.stream_export.set_by_yaml_kv(k, v),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        self.stream_export.check(self.name.clone())?;
        Ok(())
    }
}

impl ExporterConfig for GraphiteExporterConfig {
    fn name(&self) -> &NodeName {
        &self.name
    }

    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    fn exporter_type(&self) -> &'static str {
        EXPORTER_CONFIG_TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saphyr::LoadableYamlNode;

    #[test]
    fn parse_exporter_config() {
        let docs = Yaml::load_from_str(
            r#"
name: g1
server: 127.0.0.1
port: 2003
emit_interval: 5s
prefix: app.metrics
"#,
        )
        .unwrap();
        let cfg = GraphiteExporterConfig::parse(docs[0].as_mapping().unwrap(), None).unwrap();
        assert_eq!(cfg.name().as_str(), "g1");
        assert_eq!(cfg.emit_interval, Duration::from_secs(5));
        assert_eq!(
            cfg.prefix.as_ref().unwrap().display('.').to_string(),
            "app.metrics"
        );
    }

    #[test]
    fn parse_requires_name_and_server() {
        let docs = Yaml::load_from_str("server: 127.0.0.1\n").unwrap();
        assert!(GraphiteExporterConfig::parse(docs[0].as_mapping().unwrap(), None).is_err());

        let docs = Yaml::load_from_str("name: g1\n").unwrap();
        assert!(GraphiteExporterConfig::parse(docs[0].as_mapping().unwrap(), None).is_err());
    }
}

#[cfg(test)]
mod diff_action_tests {
    use saphyr::Yaml;

    use vey_yaml::yaml_doc;

    use super::GraphiteExporterConfig;
    use crate::config::exporter::{AnyExporterConfig, ExporterConfigDiffAction};

    fn parse(doc: &Yaml) -> GraphiteExporterConfig {
        let Yaml::Mapping(map) = doc else {
            panic!("yaml root should be a map");
        };
        GraphiteExporterConfig::parse(map, None).unwrap()
    }

    #[test]
    fn same_yaml() {
        let doc = yaml_doc!(
            r#"
            name: diff-a
            server: 127.0.0.1
            port: 2003
            "#
        );
        let old = parse(&doc);
        let new = parse(&doc);
        assert!(matches!(
            old.diff_action(&AnyExporterConfig::Graphite(new)),
            ExporterConfigDiffAction::NoAction
        ));
    }
}
