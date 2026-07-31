/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2023-2025 ByteDance and/or its affiliates.
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use ascii::AsciiString;
use slog::Logger;
use yaml_rust::{Yaml, yaml};

use vey_histogram::HistogramMetricsConfig;
use vey_types::metrics::{MetricTagMap, NodeName};
use vey_types::net::{OpensslServerConfigBuilder, TcpListenConfig};
use vey_yaml::YamlDocPosition;

use super::{AnyKeyServerConfig, KeyServerConfig, KeyServerConfigDiffAction};

pub(crate) const SERVER_CONFIG_TYPE: &str = "KeylessCf";

#[derive(Clone)]
pub(crate) struct KeylessCfServerConfig {
    name: NodeName,
    position: Option<YamlDocPosition>,
    pub(crate) shared_logger: Option<AsciiString>,
    pub(crate) listen: Option<TcpListenConfig>,
    pub(crate) tls_server: Option<OpensslServerConfigBuilder>,
    pub(crate) multiplex_queue_depth: usize,
    pub(crate) request_read_timeout: Duration,
    pub(crate) duration_stats: HistogramMetricsConfig,
    #[cfg(feature = "openssl-async-job")]
    pub(crate) async_op_timeout: Duration,
    pub(crate) concurrency_limit: usize,
    pub(crate) extra_metrics_tags: Option<Arc<MetricTagMap>>,
}

impl KeylessCfServerConfig {
    fn new(position: Option<YamlDocPosition>) -> Self {
        KeylessCfServerConfig {
            name: NodeName::default(),
            position,
            shared_logger: None,
            listen: None,
            tls_server: None,
            multiplex_queue_depth: 0,
            request_read_timeout: Duration::from_millis(100),
            duration_stats: HistogramMetricsConfig::default(),
            #[cfg(feature = "openssl-async-job")]
            async_op_timeout: Duration::from_secs(1),
            concurrency_limit: 0,
            extra_metrics_tags: None,
        }
    }

    pub(crate) fn parse(
        map: &yaml::Hash,
        position: Option<YamlDocPosition>,
    ) -> anyhow::Result<Self> {
        let mut server = KeylessCfServerConfig::new(position);

        vey_yaml::foreach_kv(map, |k, v| server.set(k, v))?;

        server.check()?;
        Ok(server)
    }

    fn check(&mut self) -> anyhow::Result<()> {
        if self.name.is_empty() {
            return Err(anyhow!("name is not set"));
        }
        if let Some(listen) = &mut self.listen {
            listen.check().context("invalid listen address")?;
        } else if self.tls_server.is_some() {
            return Err(anyhow!("tls_server requires listen to be set"));
        }
        Ok(())
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            super::CONFIG_KEY_SERVER_TYPE => Ok(()),
            super::CONFIG_KEY_SERVER_NAME => {
                self.name = vey_yaml::value::as_metric_node_name(v)?;
                Ok(())
            }
            "shared_logger" => {
                let name = vey_yaml::value::as_ascii(v)?;
                self.shared_logger = Some(name);
                Ok(())
            }
            "extra_metrics_tags" => {
                let tags = vey_yaml::value::as_static_metrics_tags(v)
                    .context(format!("invalid static metrics tags value for key {k}"))?;
                self.extra_metrics_tags = Some(Arc::new(tags));
                Ok(())
            }
            "listen" => {
                let listen = vey_yaml::value::as_tcp_listen_config(v)
                    .context(format!("invalid tcp listen config value for key {k}"))?;
                self.listen = Some(listen);
                Ok(())
            }
            "tls" | "tls_server" => {
                let lookup_dir = vey_daemon::config::get_lookup_dir(self.position.as_ref())?;
                let tls_server =
                    vey_yaml::value::as_openssl_tls_server_config_builder(v, Some(lookup_dir))
                        .context(format!("invalid server tls config value for key {k}"))?;
                self.tls_server = Some(tls_server);
                Ok(())
            }
            "multiplex_queue_depth" => {
                self.multiplex_queue_depth = vey_yaml::value::as_usize(v)?;
                Ok(())
            }
            "request_read_timeout" => {
                self.request_read_timeout = vey_yaml::humanize::as_duration(v)?;
                Ok(())
            }
            "duration_stats" | "duration_metrics" => {
                self.duration_stats = vey_yaml::value::as_histogram_metrics_config(v).context(
                    format!("invalid histogram metrics config value for key {k}"),
                )?;
                Ok(())
            }
            #[cfg(feature = "openssl-async-job")]
            "async_op_timeout" => {
                self.async_op_timeout = vey_yaml::humanize::as_duration(v)?;
                Ok(())
            }
            "concurrency_limit" => {
                self.concurrency_limit = vey_yaml::value::as_usize(v)?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }

    pub(crate) fn get_task_logger(&self) -> Option<Logger> {
        if let Some(shared_logger) = &self.shared_logger {
            crate::log::task::get_shared_logger(shared_logger.as_str(), self.name())
        } else {
            crate::log::task::get_logger(self.name())
        }
    }

    pub(crate) fn get_request_logger(&self) -> Option<Logger> {
        if let Some(shared_logger) = &self.shared_logger {
            crate::log::request::get_shared_logger(shared_logger.as_str(), self.name())
        } else {
            crate::log::request::get_logger(self.name())
        }
    }
}

impl KeyServerConfig for KeylessCfServerConfig {
    #[inline]
    fn name(&self) -> &NodeName {
        &self.name
    }

    #[inline]
    fn position(&self) -> Option<YamlDocPosition> {
        self.position.clone()
    }

    #[inline]
    fn r#type(&self) -> &'static str {
        SERVER_CONFIG_TYPE
    }

    fn diff_action(&self, new: &AnyKeyServerConfig) -> KeyServerConfigDiffAction {
        let AnyKeyServerConfig::KeylessCf(_) = new else {
            return KeyServerConfigDiffAction::SpawnNew;
        };

        // the openssl based tls config can not be compared, always respawn
        KeyServerConfigDiffAction::ReloadAndRespawn
    }
}
