/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;

use anyhow::anyhow;
use jiff::Timestamp;
use tokio::sync::mpsc;

use vey_types::metrics::NodeName;

use super::{ArcExporterInternal, Exporter, ExporterInternal};
use crate::config::exporter::prometheus_push::PrometheusPushExporterConfig;
use crate::config::exporter::{AnyExporterConfig, ExporterConfig};
use crate::runtime::export::{AggregateExportRuntime, HttpExportRuntime};
use crate::types::MetricRecord;

mod format;
use format::{PrometheusAggregateExport, PrometheusHttpExport};

pub(crate) struct PrometheusPushExporter {
    config: PrometheusPushExporterConfig,
    sender: mpsc::UnboundedSender<(Timestamp, MetricRecord)>,
}

impl PrometheusPushExporter {
    fn new(config: PrometheusPushExporterConfig) -> anyhow::Result<Self> {
        let (sender, receiver) = mpsc::unbounded_channel();
        let (agg_sender, agg_receiver) = mpsc::unbounded_channel();
        let aggregate_export = PrometheusAggregateExport::new(&config, agg_sender);
        let aggregate_runtime = AggregateExportRuntime::new(aggregate_export, receiver);

        let http_export = PrometheusHttpExport::new(&config)?;
        let http_runtime =
            HttpExportRuntime::new(config.http_export.clone(), http_export, agg_receiver);

        tokio::spawn(async move { aggregate_runtime.into_running().await });
        tokio::spawn(http_runtime.into_running());
        Ok(PrometheusPushExporter { config, sender })
    }

    pub(crate) fn prepare_initial(
        config: PrometheusPushExporterConfig,
    ) -> anyhow::Result<ArcExporterInternal> {
        let server = PrometheusPushExporter::new(config)?;
        Ok(Arc::new(server))
    }

    fn prepare_reload(&self, config: AnyExporterConfig) -> anyhow::Result<PrometheusPushExporter> {
        if let AnyExporterConfig::Prometheus(config) = config {
            PrometheusPushExporter::new(config)
        } else {
            Err(anyhow!(
                "config type mismatch: expect {}, actual {}",
                self.config.exporter_type(),
                config.exporter_type()
            ))
        }
    }
}

impl Exporter for PrometheusPushExporter {
    #[inline]
    fn name(&self) -> &NodeName {
        self.config.name()
    }

    #[inline]
    fn r#type(&self) -> &'static str {
        self.config.exporter_type()
    }

    fn add_metric(&self, time: Timestamp, record: &MetricRecord) {
        let _ = self.sender.send((time, record.clone())); // TODO record drop
    }
}

impl ExporterInternal for PrometheusPushExporter {
    fn _clone_config(&self) -> AnyExporterConfig {
        AnyExporterConfig::Prometheus(self.config.clone())
    }

    fn _reload(&self, config: AnyExporterConfig) -> anyhow::Result<ArcExporterInternal> {
        let exporter = self.prepare_reload(config)?;
        Ok(Arc::new(exporter))
    }
}
