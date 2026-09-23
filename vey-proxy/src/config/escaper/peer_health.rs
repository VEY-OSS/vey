/*
 * SPDX-License-Identifier: Apache-2.0
 * SPDX-FileCopyrightText: 2026 VEY-OSS Developers.
 */

use std::time::Duration;

use anyhow::anyhow;
use yaml_rust::Yaml;

/// Passive peer health check, in the same shape as nginx `max_fails` / `fail_timeout`.
///
/// A peer is treated as unavailable after `max_fails` connect failures inside one
/// `fail_timeout` window, and stays unavailable for another `fail_timeout`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PeerHealthCheckConfig {
    pub(crate) max_fails: u32,
    pub(crate) fail_timeout: Duration,
}

impl Default for PeerHealthCheckConfig {
    fn default() -> Self {
        PeerHealthCheckConfig {
            max_fails: 1,
            fail_timeout: Duration::from_secs(10),
        }
    }
}

impl PeerHealthCheckConfig {
    pub(crate) fn parse(v: &Yaml) -> anyhow::Result<Self> {
        let Yaml::Hash(map) = v else {
            return Err(anyhow!(
                "yaml value type for 'peer_health_check' should be 'map'"
            ));
        };

        let mut config = Self::default();
        vey_yaml::foreach_kv(map, |k, v| config.set(k, v))?;
        Ok(config)
    }

    fn set(&mut self, k: &str, v: &Yaml) -> anyhow::Result<()> {
        match vey_yaml::key::normalize(k).as_str() {
            "max_fails" => {
                let max_fails = vey_yaml::value::as_u32(v)?;
                if max_fails == 0 {
                    return Err(anyhow!("max_fails must be greater than 0"));
                }
                self.max_fails = max_fails;
                Ok(())
            }
            "fail_timeout" => {
                let timeout = vey_yaml::humanize::as_duration(v)?;
                if timeout.is_zero() {
                    return Err(anyhow!("fail_timeout must be greater than 0"));
                }
                self.fail_timeout = timeout;
                Ok(())
            }
            _ => Err(anyhow!("invalid key {k}")),
        }
    }
}
