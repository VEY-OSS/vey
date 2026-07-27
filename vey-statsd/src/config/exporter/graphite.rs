/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, anyhow};
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::{MetricTagMap, NodeName};
use vey_yaml::YamlDocPosition;

use super::{AnyExporterConfig, ExporterConfig, ExporterConfigDiffAction};
use crate::runtime::export::StreamExportConfig;
use crate::types::MetricName;

const EXPORTER_CONFIG_TYPE: &str = "Graphite";

/// Which aggregate counter field Graphite plaintext export should send.
///
/// Graphite accepts a single numeric value per line, so callers choose either
/// the lifetime total (`sum`) or the current emit-interval delta (`diff`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum GraphiteCounterValue {
    /// Lifetime cumulative counter (historical default).
    #[default]
    Sum,
    /// Count accumulated only in the current emit interval.
    Diff,
}

impl FromStr for GraphiteCounterValue {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "sum" => Ok(Self::Sum),
            "diff" => Ok(Self::Diff),
            _ => Err(anyhow!("invalid graphite counter value: {s}")),
        }
    }
}

impl GraphiteCounterValue {
    pub(crate) fn parse_yaml(value: &Yaml) -> anyhow::Result<Self> {
        if let Yaml::String(s) = value {
            Self::from_str(s)
        } else {
            Err(anyhow!(
                "yaml value type for graphite counter_value should be string"
            ))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct GraphiteExporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) emit_interval: Duration,
    pub(crate) stream_export: StreamExportConfig,
    pub(crate) prefix: Option<MetricName>,
    pub(crate) global_tags: MetricTagMap,
    pub(crate) counter_value: GraphiteCounterValue,
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
            counter_value: GraphiteCounterValue::default(),
        }
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
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
            "counter_value" => {
                self.counter_value = GraphiteCounterValue::parse_yaml(v)
                    .context(format!("invalid value for key {k}"))?;
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

    fn diff_action(&self, new: &AnyExporterConfig) -> ExporterConfigDiffAction {
        let AnyExporterConfig::Graphite(_new) = new else {
            return ExporterConfigDiffAction::SpawnNew;
        };

        ExporterConfigDiffAction::Reload
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust::YamlLoader;

    #[test]
    fn parse_counter_value() {
        assert_eq!(
            GraphiteCounterValue::from_str("sum").unwrap(),
            GraphiteCounterValue::Sum
        );
        assert_eq!(
            GraphiteCounterValue::from_str("DIFF").unwrap(),
            GraphiteCounterValue::Diff
        );
        assert!(GraphiteCounterValue::from_str("rate").is_err());
        assert_eq!(
            GraphiteCounterValue::parse_yaml(&Yaml::String("diff".into())).unwrap(),
            GraphiteCounterValue::Diff
        );
        assert!(GraphiteCounterValue::parse_yaml(&Yaml::Boolean(true)).is_err());
    }

    #[test]
    fn parse_exporter_config() {
        let docs = YamlLoader::load_from_str(
            r#"
name: g1
server: 127.0.0.1
port: 2003
emit_interval: 5s
prefix: app.metrics
counter_value: diff
"#,
        )
        .unwrap();
        let cfg = GraphiteExporterConfig::parse(docs[0].as_hash().unwrap(), None).unwrap();
        assert_eq!(cfg.name().as_str(), "g1");
        assert_eq!(cfg.emit_interval, Duration::from_secs(5));
        assert_eq!(cfg.counter_value, GraphiteCounterValue::Diff);
        assert_eq!(
            cfg.prefix.as_ref().unwrap().display('.').to_string(),
            "app.metrics"
        );
    }

    #[test]
    fn parse_requires_name_and_server() {
        let docs = YamlLoader::load_from_str("server: 127.0.0.1\n").unwrap();
        assert!(GraphiteExporterConfig::parse(docs[0].as_hash().unwrap(), None).is_err());

        let docs = YamlLoader::load_from_str("name: g1\n").unwrap();
        assert!(GraphiteExporterConfig::parse(docs[0].as_hash().unwrap(), None).is_err());
    }
}
