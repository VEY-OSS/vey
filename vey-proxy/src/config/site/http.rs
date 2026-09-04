/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::{Context, anyhow};
use yaml_rust::Yaml;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SiteHttpConfig {
    pub(crate) rsp_hdr_recv_timeout: Option<Duration>,
}

impl SiteHttpConfig {
    pub(crate) fn parse_yaml(&mut self, value: &Yaml) -> anyhow::Result<()> {
        let Yaml::Hash(map) = value else {
            return Err(anyhow!("yaml value type for 'site http' should be 'map'"));
        };
        vey_yaml::foreach_kv(map, |k, v| self.set(k, v))
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "rsp_header_recv_timeout" => {
                let timeout = vey_yaml::humanize::as_duration(v)
                    .context(format!("invalid humanize duration value for key {k}"))?;
                self.rsp_hdr_recv_timeout = Some(timeout);
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}
