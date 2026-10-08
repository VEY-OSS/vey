/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, anyhow};
use http::uri::PathAndQuery;
use yaml_rust::{Yaml, yaml};

use vey_types::metrics::{MetricTagMap, NodeName};
use vey_yaml::YamlDocPosition;

use super::{AnyExporterConfig, ExporterConfig, ExporterConfigDiffAction};
use crate::runtime::export::HttpExportConfig;
use crate::types::MetricName;

const EXPORTER_CONFIG_TYPE: &str = "PrometheusPush";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PrometheusPushExporterConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) emit_interval: Duration,
    pub(crate) max_samples: usize,
    pub(crate) http_export: HttpExportConfig,
    path: String,
    pub(crate) bearer_token: String,
    pub(crate) org_id: String,
    pub(crate) prefix: Option<MetricName>,
    pub(crate) global_tags: MetricTagMap,
}

impl PrometheusPushExporterConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        PrometheusPushExporterConfig {
            name: NodeName::default(),
            position,
            emit_interval: Duration::from_secs(10),
            max_samples: 10000,
            http_export: HttpExportConfig::new(9090),
            path: "/api/v1/write".to_string(),
            bearer_token: String::new(),
            org_id: String::new(),
            prefix: None,
            global_tags: MetricTagMap::default(),
        }
    }

    pub(crate) fn build_api_path(&self) -> anyhow::Result<PathAndQuery> {
        PathAndQuery::from_str(&self.path)
            .map_err(|e| anyhow!("invalid prometheus_push api path {}: {e}", self.path))
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut collector = PrometheusPushExporterConfig::new(position);

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
            "max_samples" => {
                self.max_samples = vey_yaml::value::as_usize(v)?;
                Ok(())
            }
            "path" => {
                self.path = vey_yaml::value::as_string(v)?;
                Ok(())
            }
            "bearer_token" => {
                self.bearer_token = vey_yaml::value::as_http_header_value_string(v)
                    .context(format!("invalid http header value string for key {k}"))?;
                Ok(())
            }
            "org_id" => {
                self.org_id = vey_yaml::value::as_http_header_value_string(v)
                    .context(format!("invalid http header value string for key {k}"))?;
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
            _ => self.http_export.set_by_yaml_kv(k, v),
        }
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if self.max_samples == 0 {
            return Err(anyhow!("max_samples must be greater than 0"));
        }
        if !self.path.starts_with('/') {
            return Err(anyhow!("path must start with /"));
        }
        self.build_api_path()?;
        self.http_export.check(self.name.clone())?;
        Ok(())
    }
}

impl ExporterConfig for PrometheusPushExporterConfig {
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
        let AnyExporterConfig::Prometheus(_new) = new else {
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
    fn parse_exporter_config() {
        let docs = YamlLoader::load_from_str(
            r#"
name: p1
server: 127.0.0.1
port: 8428
emit_interval: 5s
max_samples: 100
path: /api/v1/write
bearer_token: secret
org_id: tenant-a
prefix: app.metrics
"#,
        )
        .unwrap();
        let cfg = PrometheusPushExporterConfig::parse(docs[0].as_hash().unwrap(), None).unwrap();
        assert_eq!(cfg.name().as_str(), "p1");
        assert_eq!(cfg.emit_interval, Duration::from_secs(5));
        assert_eq!(cfg.max_samples, 100);
        assert_eq!(cfg.build_api_path().unwrap().as_str(), "/api/v1/write");
        assert_eq!(cfg.bearer_token, "secret");
        assert_eq!(cfg.org_id, "tenant-a");
        assert_eq!(
            cfg.prefix.as_ref().unwrap().display('.').to_string(),
            "app.metrics"
        );
    }

    #[test]
    fn reject_path_without_slash() {
        let docs = YamlLoader::load_from_str(
            r#"
name: p1
server: 127.0.0.1
path: api/v1/push
"#,
        )
        .unwrap();
        assert!(PrometheusPushExporterConfig::parse(docs[0].as_hash().unwrap(), None).is_err());
    }
}
