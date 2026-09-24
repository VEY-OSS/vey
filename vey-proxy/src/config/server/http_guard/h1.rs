/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::num::NonZeroUsize;
use std::time::Duration;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HttpGuardH1Config {
    pub(crate) pipeline_size: NonZeroUsize,
    pub(crate) pipeline_read_idle_timeout: Duration,
    pub(crate) body_line_max_len: usize,
}

impl Default for HttpGuardH1Config {
    fn default() -> Self {
        HttpGuardH1Config {
            pipeline_size: NonZeroUsize::new(10).unwrap(),
            pipeline_read_idle_timeout: Duration::from_secs(300),
            body_line_max_len: 8192,
        }
    }
}

impl HttpGuardH1Config {
    pub(super) fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("yaml value type for 'h1' should be 'map'"));
        };
        vey_yaml::foreach_kv(map, |k, v| self.set(k, v))
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "pipeline_size" => {
                self.pipeline_size = vey_yaml::value::as_nonzero_usize(v)
                    .context(format!("invalid nonzero usize value for key {k}"))?;
                Ok(())
            }
            "pipeline_read_idle_timeout" => {
                self.pipeline_read_idle_timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                Ok(())
            }
            "body_line_max_length" => {
                self.body_line_max_len = vey_yaml::value::as_usize(v)
                    .context(format!("invalid usize value for key {k}"))?;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}
